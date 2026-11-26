use guild_session::workflow::validate_definition;
use serde_json::{Value, json};

fn definition() -> Value {
    json!({
        "version": 1, "id": "example", "label": "A configurable process",
        "roles": {"parent": {"label": "Coordinator", "owner": "parent"}},
        "outcomes": {"contract": {"validator": "contract", "required": true}},
        "steps": [{"id": "prepare", "label": "Prepare contract", "action": "contract.prepare",
            "role": "parent", "depends_on": [], "outcomes": ["contract"]}],
        "skills": [], "corrections": [],
        "limits": {"review_calls": 18, "correction_rounds": 3}
    })
}

#[test]
fn workflow_accepts_a_configured_graph_without_launching_anything() {
    let input = definition();
    let accepted = validate_definition(&serde_json::to_vec(&input).unwrap()).unwrap();
    assert_eq!(accepted["steps"][0]["label"], "Prepare contract");
}

#[test]
fn workflow_refuses_unknown_actions_and_broken_dependencies_before_effects() {
    for (field, value, diagnostic) in [
        ("action", json!("shell.execute"), "action"),
        ("depends_on", json!(["missing"]), "depends_on"),
        ("role", json!("missing"), "role"),
    ] {
        let mut input = definition();
        input["steps"][0][field] = value;
        let error = validate_definition(&serde_json::to_vec(&input).unwrap()).unwrap_err();
        assert!(error.contains(diagnostic), "{error}");
    }
}

#[test]
fn workflow_default_and_override_graphs_use_data_order_and_preserve_zero() {
    use guild_session::workflow::Definition;
    let default = Definition::shipped().unwrap();
    assert_eq!(default.limits.review_calls, 18);
    assert_eq!(default.order().unwrap().last().unwrap().id, "delivery");
    let project = serde_json::to_vec(&json!({"label":"Project flow", "limits":{"review_calls":0,"correction_rounds":0}, "corrections":[]})).unwrap();
    let explicit = serde_json::to_vec(
        &json!({"label":"Selected flow", "outcomes":{"delivery":{"required":false}}}),
    )
    .unwrap();
    let selected = Definition::resolve(Some(&project), Some(&explicit)).unwrap();
    assert_eq!(selected.label, "Selected flow");
    assert_eq!(selected.limits.review_calls, 0);
    assert!(!selected.outcomes["delivery"].required);
    let mut graph = definition();
    graph["steps"] = json!([
        {"id":"later","label":"Second","action":"checkpoint.decide","role":"parent","depends_on":["prepare"],"outcomes":[]},
        graph["steps"][0].clone()
    ]);
    let custom = Definition::parse(&serde_json::to_vec(&graph).unwrap()).unwrap();
    assert_eq!(
        custom
            .order()
            .unwrap()
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["prepare", "later"]
    );
}

#[test]
fn workflow_refuses_ambiguous_and_unbounded_configuration() {
    let original = definition();
    for (input, path) in [
        (
            {
                let mut v = original.clone();
                v["version"] = json!(2);
                v
            },
            "version",
        ),
        (
            {
                let mut v = original.clone();
                v["id"] = json!("../escape");
                v
            },
            "id",
        ),
        (
            {
                let mut v = original.clone();
                let second = v["steps"][0].clone();
                v["steps"].as_array_mut().unwrap().push(second);
                v
            },
            "id",
        ),
        (
            {
                let mut v = original.clone();
                v["steps"][0]["condition"] = json!({"op":"eval","script":"x"});
                v
            },
            "condition",
        ),
        (
            {
                let mut v = original.clone();
                v["limits"]["correction_rounds"] = json!(101);
                v
            },
            "limits",
        ),
        (
            {
                let mut v = original.clone();
                v["steps"][0]["action"] = json!("review.request");
                v
            },
            "role",
        ),
    ] {
        let error = validate_definition(&serde_json::to_vec(&input).unwrap()).unwrap_err();
        assert!(error.contains(path), "expected {path}: {error}");
    }
    let duplicated = serde_json::to_string(&original)
        .unwrap()
        .replace("\"version\":1", "\"version\":1,\"version\":1");
    assert!(validate_definition(duplicated.as_bytes()).is_err());
}

#[test]
fn workflow_does_not_silently_replace_duplicate_role_ids() {
    let input = serde_json::to_string(&definition()).unwrap();
    let duplicate = input.replace(
        "\"parent\":{",
        "\"parent\":{\"label\":\"Other\",\"owner\":\"managed\"},\"parent\":{",
    );
    assert!(
        validate_definition(duplicate.as_bytes()).is_err(),
        "duplicate role identity was silently replaced"
    );
}

#[test]
fn lightweight_contract_never_initializes_openspec_or_requires_superpowers() {
    use guild_session::workflow::{ContractKind, pin_contract};
    let target = tempfile::tempdir().unwrap();
    std::fs::write(target.path().join("task.md"), "Accepted lightweight task").unwrap();
    assert_eq!(
        pin_contract(target.path(), Some(std::path::Path::new("task.md")), None)
            .unwrap()
            .kind,
        ContractKind::Lightweight
    );
    assert!(!target.path().join("openspec").exists());
}

#[test]
fn openspec_selection_waits_for_tools_and_ambiguity_and_pins_exact_artifacts() {
    use guild_session::workflow::{ContractKind, pin_contract};
    use std::path::Path;
    let target = tempfile::tempdir().unwrap();
    let change = Path::new("openspec/changes/selected");
    let directory = target.path().join(change);
    std::fs::create_dir_all(directory.join("specs/flow")).unwrap();
    for name in [
        "proposal.md",
        "design.md",
        "tasks.md",
        "acceptance.md",
        "specs/flow/spec.md",
    ] {
        std::fs::write(directory.join(name), "Accepted bytes").unwrap();
    }
    assert!(
        pin_contract(target.path(), Some(change), None)
            .unwrap_err()
            .contains("CLI")
    );
    let pinned = pin_contract(
        target.path(),
        Some(change),
        Some(Path::new("/usr/bin/true")),
    )
    .unwrap();
    assert_eq!(pinned.kind, ContractKind::Openspec);
    assert!(pinned.skipped.is_empty());
    assert_eq!(pinned.artifacts.len(), 5);
    pinned.verify().unwrap();
    std::fs::write(directory.join("specs/flow/added.md"), "Unreviewed addition").unwrap();
    assert!(
        pinned.verify().is_err(),
        "new delta bytes must invalidate the pinned contract"
    );
    std::fs::remove_file(directory.join("specs/flow/added.md")).unwrap();
    std::fs::write(directory.join("design.md"), "Changed design").unwrap();
    assert!(
        pinned
            .verify()
            .unwrap_err()
            .to_string()
            .contains("design.md")
    );
    std::fs::create_dir_all(target.path().join("openspec/changes/other")).unwrap();
    assert!(
        pin_contract(target.path(), None, Some(Path::new("/usr/bin/true")))
            .unwrap_err()
            .contains("2 candidates")
    );
    assert!(
        pin_contract(
            target.path(),
            Some(Path::new("../escape")),
            Some(Path::new("/usr/bin/true"))
        )
        .is_err()
    );
}

#[test]
fn skill_snapshot_prefers_project_alias_and_preserves_relative_resources() {
    use guild_session::workflow::{SkillReference, SkillScope, snapshot_skill};
    let tmp = tempfile::tempdir().unwrap();
    let _cleanup = WritableSnapshot(tmp.path().join("snapshots"));
    let project = tmp.path().join("project");
    let global = tmp.path().join("global");
    std::fs::create_dir_all(project.join(".claude/skills/example/references")).unwrap();
    std::fs::create_dir_all(project.join(".agents")).unwrap();
    std::os::unix::fs::symlink("../.claude/skills", project.join(".agents/skills")).unwrap();
    std::fs::create_dir_all(global.join("example")).unwrap();
    std::fs::write(
        project.join(".claude/skills/example/SKILL.md"),
        "Project [resource](references/guide.md)",
    )
    .unwrap();
    std::fs::write(
        project.join(".claude/skills/example/references/guide.md"),
        "Pinned relative bytes",
    )
    .unwrap();
    std::fs::write(global.join("example/SKILL.md"), "Global bytes").unwrap();
    let reference = SkillReference {
        id: "example".into(),
        name: "example".into(),
        scope: SkillScope::Auto,
        required: true,
    };
    let pinned = snapshot_skill(
        &reference,
        &[project.join(".agents/skills")],
        &[global],
        &tmp.path().join("snapshots"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(pinned.directory.join("references/guide.md")).unwrap(),
        "Pinned relative bytes"
    );
    std::fs::write(
        project.join(".claude/skills/example/references/guide.md"),
        "Changed original",
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(pinned.directory.join("references/guide.md")).unwrap(),
        "Pinned relative bytes"
    );
}

// Snapshots intentionally retain read-only directories in production; a private
// fixture restores its own permissions before TempDir removes its resources.
struct WritableSnapshot(std::path::PathBuf);
impl Drop for WritableSnapshot {
    fn drop(&mut self) {
        fn writable(path: &std::path::Path) {
            use std::os::unix::fs::PermissionsExt;
            let Ok(meta) = std::fs::symlink_metadata(path) else {
                return;
            };
            if meta.file_type().is_symlink() {
                return;
            }
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
            if meta.is_dir() {
                for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
                    writable(&entry.path());
                }
            }
        }
        writable(&self.0);
    }
}

#[test]
fn skills_report_missing_optional_required_and_unsafe_links_without_installation() {
    use guild_session::workflow::{SkillReference, SkillScope, snapshot_skill};
    let tmp = tempfile::tempdir().unwrap();
    let _cleanup = WritableSnapshot(tmp.path().join("snapshots"));
    let roots = vec![tmp.path().join("skills")];
    let mut reference = SkillReference {
        id: "example".into(),
        name: "example".into(),
        scope: SkillScope::Project,
        required: true,
    };
    assert!(
        snapshot_skill(&reference, &roots, &[], &tmp.path().join("snapshots"))
            .unwrap_err()
            .contains("required skill example")
    );
    reference.required = false;
    assert!(
        snapshot_skill(&reference, &roots, &[], &tmp.path().join("snapshots"))
            .unwrap_err()
            .contains("optional skill example")
    );
    assert!(!roots[0].exists());
    let skill = roots[0].join("example");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "Instructions").unwrap();
    std::fs::write(tmp.path().join("outside"), "Private outside bytes").unwrap();
    for (target, diagnostic) in [
        (tmp.path().join("outside"), "escapes"),
        (skill.join("missing"), "dangling"),
        (skill.clone(), "cyclic"),
    ] {
        std::os::unix::fs::symlink(target, skill.join("link")).unwrap();
        assert!(
            snapshot_skill(&reference, &roots, &[], &tmp.path().join("snapshots"))
                .unwrap_err()
                .contains(diagnostic)
        );
        std::fs::remove_file(skill.join("link")).unwrap();
    }
    std::fs::write(
        skill.join("SKILL.md"),
        "[external][file]\n\n[file]: ../../outside\n",
    )
    .unwrap();
    assert!(
        snapshot_skill(&reference, &roots, &[], &tmp.path().join("snapshots"))
            .unwrap_err()
            .contains("escapes")
    );
    std::fs::write(
        skill.join("SKILL.md"),
        "`[example](../../outside)`\n\n[real](guide.md \"Title\")",
    )
    .unwrap();
    std::fs::write(skill.join("guide.md"), "Accessible resource").unwrap();
    let pinned = snapshot_skill(&reference, &roots, &[], &tmp.path().join("snapshots")).unwrap();
    pinned.verify().unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(pinned.directory.join("guide.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o222,
        0
    );
    assert_eq!(pinned.scope, SkillScope::Project);
}

#[test]
fn contained_directory_links_preserve_parent_relative_resource_meaning() {
    use guild_session::workflow::{SkillReference, SkillScope, snapshot_skill};
    let tmp = tempfile::tempdir().unwrap();
    let _cleanup = WritableSnapshot(tmp.path().join("snapshots"));
    let skill = tmp.path().join("skills/example");
    std::fs::create_dir_all(skill.join("refs/deep")).unwrap();
    std::fs::write(skill.join("SKILL.md"), "[Guide](alias/guide.md)").unwrap();
    std::fs::write(
        skill.join("refs/deep/guide.md"),
        "[Parent resource](../shared.md)",
    )
    .unwrap();
    std::fs::write(skill.join("refs/shared.md"), "Correct related resource").unwrap();
    std::fs::write(skill.join("shared.md"), "Different root resource").unwrap();
    std::os::unix::fs::symlink("refs/deep", skill.join("alias")).unwrap();
    let reference = SkillReference {
        id: "example".into(),
        name: "example".into(),
        scope: SkillScope::Project,
        required: true,
    };
    let pinned = snapshot_skill(
        &reference,
        &[tmp.path().join("skills")],
        &[],
        &tmp.path().join("snapshots"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(pinned.directory.join("alias/../shared.md")).unwrap(),
        "Correct related resource"
    );
}

#[test]
fn skill_snapshot_retains_executable_resources_and_detects_corruption() {
    use guild_session::workflow::{SkillReference, SkillScope, snapshot_skill};
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let _cleanup = WritableSnapshot(tmp.path().join("snapshots"));
    let skill = tmp.path().join("skills/example");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "[Fixture](check.sh)").unwrap();
    std::fs::write(
        skill.join("check.sh"),
        "#!/bin/sh\nprintf 'pinned executable'\n",
    )
    .unwrap();
    std::fs::set_permissions(
        skill.join("check.sh"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let reference = SkillReference {
        id: "example".into(),
        name: "example".into(),
        scope: SkillScope::Project,
        required: true,
    };
    let pinned = snapshot_skill(
        &reference,
        &[tmp.path().join("skills")],
        &[],
        &tmp.path().join("snapshots"),
    )
    .unwrap();
    let output = std::process::Command::new(pinned.directory.join("check.sh"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"pinned executable");
    std::fs::set_permissions(
        pinned.directory.join("check.sh"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    std::fs::write(pinned.directory.join("check.sh"), "Changed snapshot").unwrap();
    assert!(
        pinned
            .verify()
            .unwrap_err()
            .to_string()
            .contains("corrupt or changed")
    );
}

#[test]
fn source_identity_changes_with_dirty_and_untracked_bytes_at_the_same_head() {
    use guild_session::workflow::{SourceFile, SourceIdentity, SourceLayer, digest};
    let target = tempfile::tempdir().unwrap();
    let make = |bytes: &[u8], untracked: &[u8]| {
        SourceIdentity::new(
            target.path().into(),
            "a".repeat(40),
            "b".repeat(40),
            vec![
                SourceFile {
                    path: "src/main.rs".into(),
                    layer: SourceLayer::Working,
                    sha256: digest(bytes),
                    mode: 0o644,
                },
                SourceFile {
                    path: "new.txt".into(),
                    layer: SourceLayer::Untracked,
                    sha256: digest(untracked),
                    mode: 0o644,
                },
            ],
        )
        .unwrap()
    };
    let before = make(b"first", b"untracked");
    let after = make(b"second", b"untracked");
    let new_untracked = make(b"first", b"changed untracked");
    assert_eq!(before.commit, new_untracked.commit);
    assert_ne!(before.sha256, new_untracked.sha256);
    assert_eq!(before.commit, after.commit);
    assert_ne!(before.sha256, after.sha256);
}

#[test]
fn evidence_requires_actual_source_producer_accessible_output_and_required_surface() {
    use guild_session::SessionId;
    use guild_session::workflow::{
        Evidence, EvidenceContext, SourceFile, SourceIdentity, SourceLayer, Surface, digest,
    };
    let target = tempfile::tempdir().unwrap();
    let producer = SessionId::new();
    let source = SourceIdentity::new(
        target.path().into(),
        "a".repeat(40),
        "b".repeat(40),
        vec![SourceFile {
            path: "file.rs".into(),
            layer: SourceLayer::Working,
            sha256: digest(b"code"),
            mode: 0o644,
        }],
    )
    .unwrap();
    let output = target.path().join("check.log");
    std::fs::write(&output, "actual fixture output").unwrap();
    let contract = digest(b"accepted contract");
    let fixture = json!({"id":uuid::Uuid::new_v4(),"producer":producer,"contract":contract,"source":source,
        "scenario":"A06","step":"accept","tool":{"program":"fixture","version":"1","argv":["fixture","check"]},
        "observed_at_ms":1,"artifacts":[{"path":output,"sha256":digest(b"actual fixture output")}],
        "observation":{"kind":"command","exit_code":0}});
    let evidence: Evidence = serde_json::from_value(fixture.clone()).unwrap();
    let roots = vec![target.path().to_path_buf()];
    let context = EvidenceContext {
        producer,
        contract: &contract,
        source: &source,
        step: "accept",
        scenario: "A06",
        surface: Surface::Command,
        expectation: guild_session::workflow::Expectation::Success,
        allowed_roots: &roots,
    };
    evidence.verify(&context).unwrap();
    for (field, value, diagnostic) in [
        ("producer", json!(SessionId::new()), "producer"),
        ("contract", json!(digest(b"other")), "contract"),
        (
            "observation",
            json!({"kind":"claim","message":"done"}),
            "claim",
        ),
    ] {
        let mut invalid = fixture.clone();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<Evidence>(invalid)
                .unwrap()
                .verify(&context)
                .unwrap_err()
                .to_string()
                .contains(diagnostic)
        );
    }
    let native_context = EvidenceContext {
        surface: Surface::Native,
        ..context
    };
    assert!(
        evidence
            .verify(&native_context)
            .unwrap_err()
            .to_string()
            .contains("surface")
    );
    let mut files = source.files.clone();
    files[0].sha256 = digest(b"different bytes");
    let changed = SourceIdentity::new(
        source.target.clone(),
        source.base.clone().unwrap(),
        source.commit.clone().unwrap(),
        files,
    )
    .unwrap();
    let stale = EvidenceContext {
        source: &changed,
        ..context
    };
    let stale_error = evidence.verify(&stale).unwrap_err();
    assert!(stale_error.to_string().contains("source"));
    assert_eq!(
        stale_error.verdict,
        guild_session::workflow::Verdict::Unverified
    );
    let mut refusal = fixture.clone();
    refusal["observation"] = json!({"kind":"command","exit_code":1});
    let refusal: Evidence = serde_json::from_value(refusal).unwrap();
    assert_eq!(
        refusal.verify(&context).unwrap_err().verdict,
        guild_session::workflow::Verdict::Fail
    );
    refusal
        .verify(&EvidenceContext {
            expectation: guild_session::workflow::Expectation::ExitCode(1),
            ..context
        })
        .unwrap();
    std::fs::write(&output, "Different output bytes").unwrap();
    assert!(
        evidence
            .verify(&context)
            .unwrap_err()
            .to_string()
            .contains("output bytes changed")
    );
    std::fs::remove_file(output).unwrap();
    assert!(
        evidence
            .verify(&context)
            .unwrap_err()
            .to_string()
            .contains("artifact")
    );
}

#[test]
fn workflow_journal_reserves_once_and_rejects_conflicting_or_stale_requests() {
    use guild_session::workflow::{Journal, Mutation, RunId};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("run");
    let run = RunId::new();
    let mut journal = Journal::create(&path, run).unwrap();
    let request = Mutation::new(
        "start-1",
        0,
        json!({"caller":"parent","operation":"assign","source":"a"}),
    )
    .unwrap();
    let (receipt, admitted) = journal.reserve(&request).unwrap();
    assert!(admitted);
    assert_eq!(receipt.run, run);
    assert_eq!(receipt.revision, 1);
    let (retry, admitted) = journal.reserve(&request).unwrap();
    assert!(!admitted);
    assert_eq!(receipt, retry);
    let mut changed = request.clone();
    changed.context["source"] = json!("b");
    assert!(journal.reserve(&changed).unwrap_err().contains("conflict"));
    let stale = Mutation::new("start-2", 0, json!({"operation":"assign"})).unwrap();
    assert!(journal.reserve(&stale).unwrap_err().contains("revision"));
    assert_eq!(journal.revision(), 1);
    assert!(
        Journal::open(&path).is_err(),
        "second window must not coordinate"
    );
    assert_eq!(
        Journal::inspect(&path).unwrap().receipts,
        vec![receipt.clone()]
    );
    drop(journal);
    let journal = Journal::open(&path).unwrap();
    assert_eq!(journal.unresolved(), vec![receipt.clone()]);
    assert_eq!(journal.receipts(), &[receipt]);
}

#[test]
fn workflow_journal_replays_committed_events_but_never_old_or_corrupt_storage() {
    use guild_session::workflow::{Journal, Mutation, RunId};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("run");
    let mut journal = Journal::create(&path, RunId::new()).unwrap();
    let state0 = std::fs::read(path.join("state.json")).unwrap();
    let request = Mutation::new("create", 0, json!({"operation":"assign"})).unwrap();
    let (receipt, _) = journal.reserve(&request).unwrap();
    drop(journal);
    // Crash after the committed event but before the replaceable snapshot.
    std::fs::write(path.join("state.json"), state0).unwrap();
    let mut journal = Journal::open(&path).unwrap();
    assert_eq!(journal.revision(), 1);
    assert_eq!(journal.unresolved(), vec![receipt.clone()]);
    journal
        .resolve(receipt.attempt, 1, json!({"delivery":"unknown"}))
        .unwrap();
    assert!(journal.unresolved().is_empty());
    drop(journal);
    let primary = std::fs::read(path.join("state.json")).unwrap();
    std::fs::write(path.join("state.json"), "{broken").unwrap();
    assert!(Journal::open(&path).is_err());
    std::fs::write(path.join("state.json"), &primary).unwrap();
    std::fs::write(path.join("state.json.bak"), &primary).unwrap();
    std::fs::remove_file(path.join("state.json")).unwrap();
    assert!(Journal::open(&path).is_err());
    std::fs::write(path.join("state.json"), primary).unwrap();
    let event = path.join("events/00000000000000000001.json");
    std::fs::write(event, "{broken").unwrap();
    assert!(Journal::open(&path).is_err());
}

#[test]
fn workflow_journal_storage_failure_withholds_effect_and_poisoned_writer_cannot_retry() {
    use guild_session::workflow::{Journal, Mutation, RunId};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("run");
    let mut journal = Journal::create(&path, RunId::new()).unwrap();
    std::fs::rename(path.join("events"), path.join("retained-events")).unwrap();
    std::fs::write(path.join("events"), "fixture denies directory access").unwrap();
    let request = Mutation::new("create", 0, json!({"operation":"assign"})).unwrap();
    assert!(journal.reserve(&request).is_err());
    std::fs::remove_file(path.join("events")).unwrap();
    std::fs::rename(path.join("retained-events"), path.join("events")).unwrap();
    assert!(
        journal.reserve(&request).is_err(),
        "uncertain writer stays blocked"
    );
    assert_eq!(journal.revision(), 0);
}

// Runs in a separate copy of this compiled test binary, never a provider process.
#[test]
fn workflow_journal_claim_process_fixture() {
    use guild_session::workflow::Journal;
    let Some(path) = std::env::var_os("GUILD_WORKFLOW_CLAIM_TEST_PATH") else {
        return;
    };
    let result = Journal::open(std::path::Path::new(&path));
    match std::env::var("GUILD_WORKFLOW_CLAIM_TEST_EXPECT")
        .unwrap()
        .as_str()
    {
        "held" => assert!(result.err().unwrap().contains("coordinator")),
        "released" => assert!(result.is_ok()),
        _ => panic!("unknown private fixture mode"),
    }
}

#[test]
fn workflow_journal_claim_is_exclusive_across_real_processes_and_released_on_exit() {
    use guild_session::workflow::{Journal, RunId};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("run");
    let journal = Journal::create(&path, RunId::new()).unwrap();
    let probe = |expected: &str| {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "workflow::workflow_journal_claim_process_fixture",
                "--nocapture",
            ])
            .env("GUILD_WORKFLOW_CLAIM_TEST_PATH", &path)
            .env("GUILD_WORKFLOW_CLAIM_TEST_EXPECT", expected)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    };
    probe("held");
    drop(journal);
    probe("released");
    // The previous subprocess has exited: kernel release, no PID-file takeover.
    assert!(Journal::open(&path).is_ok());
}

#[test]
fn workflow_run_reduces_configured_dependencies_and_never_turn_completion_into_outcomes() {
    use guild_session::{SessionId, workflow::*};
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("observed.log");
    std::fs::write(&output, "actual controlled observation").unwrap();
    let source = |text: &[u8]| {
        SourceIdentity::unversioned(
            root.path().into(),
            vec![SourceFile {
                path: "code".into(),
                layer: SourceLayer::Working,
                sha256: digest(text),
                mode: 0o644,
            }],
        )
        .unwrap()
    };
    let producer = SessionId::new();
    let contract = digest(b"accepted contract");
    let roots = [root.path().to_path_buf()];
    let checked = |step: &str, source: &SourceIdentity| {
        let evidence: Evidence = serde_json::from_value(json!({
            "id":uuid::Uuid::new_v4(),"producer":producer,"contract":contract,"source":source,
            "scenario":"controlled-command","step":step,"tool":{"program":"fixture","version":"1","argv":["fixture"]},
            "observed_at_ms":1,"artifacts":[{"path":output,"sha256":digest(b"actual controlled observation")}],
            "observation":{"kind":"command","exit_code":0}
        })).unwrap();
        QualifiedOutcome::check(
            step,
            step,
            evidence,
            &EvidenceContext {
                producer,
                contract: &contract,
                source,
                step,
                scenario: "controlled-command",
                surface: Surface::Command,
                expectation: Expectation::Success,
                allowed_roots: &roots,
            },
        )
        .unwrap()
    };
    // A configured fixture graph uses recorded command observations at these nodes.
    let mut definition = Definition::shipped().unwrap();
    for outcome in definition.outcomes.values_mut() {
        outcome.validator = Validator::Observation;
    }
    let mut graph = RunGraph::new(definition, false).unwrap();
    assert_eq!(graph.nodes()[0].state, NodeState::Ready);
    assert_eq!(graph.nodes()[1].state, NodeState::Blocked);
    graph.activity("contract", "turn completed").unwrap();
    assert_eq!(graph.nodes()[0].state, NodeState::Ready);
    graph.qualify(checked("contract", &source(b"a"))).unwrap();
    assert_eq!(graph.nodes()[0].state, NodeState::Finished);
    assert_eq!(graph.nodes()[1].state, NodeState::Ready);
    assert!(graph.qualify(checked("build", &source(b"a"))).is_err());
    graph
        .qualify(checked("plan-review", &source(b"a")))
        .unwrap();
    graph.qualify(checked("build", &source(b"b"))).unwrap();
    graph
        .qualify(checked("code-review", &source(b"b")))
        .unwrap();
    let mut design = graph.clone();
    design.correct("plan-review", "contract").unwrap();
    assert_eq!(design.nodes()[0].state, NodeState::Ready);
    assert_eq!(design.nodes()[1].state, NodeState::Blocked);
    let revision = digest(b"corrected sole contract");
    let revised_source = source(b"a");
    let mut evidence = checked("contract", &revised_source).evidence().clone();
    evidence.contract = revision.clone();
    let revised = QualifiedOutcome::check(
        "contract",
        "contract",
        evidence,
        &EvidenceContext {
            producer,
            contract: &revision,
            source: &revised_source,
            step: "contract",
            scenario: "controlled-command",
            surface: Surface::Command,
            expectation: Expectation::Success,
            allowed_roots: &roots,
        },
    )
    .unwrap();
    design.qualify(revised).unwrap();
    assert_eq!(design.nodes()[0].state, NodeState::Finished);
    assert_eq!(
        design.nodes()[1].state,
        NodeState::Ready,
        "old plan review cannot qualify the corrected contract"
    );
    assert_eq!(
        design.history().len(),
        5,
        "prior contract evidence remains historical"
    );
    let mut conditional = graph.definition().clone();
    let accept = conditional
        .steps
        .iter_mut()
        .find(|s| s.id == "accept")
        .unwrap();
    accept.depends_on.clear();
    accept.condition = Some(Condition::Not {
        condition: Box::new(Condition::Not {
            condition: Box::new(Condition::Outcome { id: "build".into() }),
        }),
    });
    let delivery = conditional
        .steps
        .iter_mut()
        .find(|s| s.id == "delivery")
        .unwrap();
    delivery.depends_on.clear();
    delivery.condition = Some(Condition::Outcome { id: "build".into() });
    let mut conditional = RunGraph::new(conditional, false).unwrap();
    for step in [
        "contract",
        "plan-review",
        "build",
        "code-review",
        "accept",
        "delivery",
    ] {
        conditional.qualify(checked(step, &source(b"b"))).unwrap();
    }
    let mut changed = conditional.clone();
    changed.invalidate_source(&source(b"changed").sha256);
    changed
        .qualify(checked("build", &source(b"changed")))
        .unwrap();
    assert_eq!(
        changed
            .nodes()
            .iter()
            .find(|n| n.step == "delivery")
            .unwrap()
            .state,
        NodeState::Ready,
        "ordinary source invalidation must invalidate condition-only consumers"
    );
    assert_eq!(
        changed
            .nodes()
            .iter()
            .find(|n| n.step == "accept")
            .unwrap()
            .state,
        NodeState::Ready
    );
    assert_eq!(
        changed.history().len(),
        7,
        "stale evidence remains inspectable"
    );
    conditional.correct("code-review", "build").unwrap();
    conditional
        .qualify(checked("build", &source(b"b")))
        .unwrap();
    assert_eq!(
        conditional
            .nodes()
            .iter()
            .find(|n| n.step == "delivery")
            .unwrap()
            .state,
        NodeState::Ready,
        "condition-dependent outcome must be observed again"
    );
    let old_build = graph.history()[2].clone();
    for cycle in 1..=3 {
        graph.correct("code-review", "build").unwrap();
        assert_eq!(graph.corrections_used(), cycle);
        assert_eq!(graph.nodes()[0].state, NodeState::Finished);
        assert_eq!(graph.nodes()[1].state, NodeState::Finished);
        assert_eq!(graph.nodes()[2].state, NodeState::Ready);
        assert_eq!(graph.nodes()[3].state, NodeState::Blocked);
        assert!(
            graph.qualify(old_build.clone()).is_err(),
            "old evidence ID cannot qualify a new correction cycle"
        );
        graph.qualify(checked("build", &source(b"b"))).unwrap();
        graph
            .qualify(checked("code-review", &source(b"b")))
            .unwrap();
    }
    let before = graph.clone();
    assert!(
        graph.correct("code-review", "build").is_err(),
        "fourth correction refused even with the same source"
    );
    assert_eq!(graph, before, "refusal must not reset history or readiness");
    assert!(
        graph.correct("plan-review", "contract").is_err(),
        "plan/code edges share the same correction ceiling"
    );
    assert_eq!(graph, before);
    assert!(
        graph.correct("accept", "contract").is_err(),
        "only configured correction edges exist"
    );
    graph.invalidate_source(&source(b"c").sha256);
    assert_ne!(graph.nodes()[3].state, NodeState::Finished);
    assert_eq!(graph.history().len(), 10);
    assert!(!graph.ready());
}

#[test]
fn configured_harness_selection_is_literal_and_never_rewrites_inherited_profile_fields() {
    use guild_session::{HarnessKind, harness::LaunchSelection};
    let selection = LaunchSelection {
        model: Some("openai/gpt-selected".into()),
        effort: Some("high".into()),
    };
    assert_eq!(
        selection.args(HarnessKind::Codex).unwrap(),
        vec![
            "--model",
            "openai/gpt-selected",
            "-c",
            "model_reasoning_effort=\"high\""
        ]
    );
    assert_eq!(
        selection.args(HarnessKind::Claude).unwrap(),
        vec!["--model", "openai/gpt-selected", "--effort", "high"]
    );
    let prior = r#"{"permissions":["keep"],"plugins":["private-plugin"],"providers":{"private":{"token_ref":"preserve-reference"}},"model":"old/provider"}"#;
    let composed = selection.opencode_inline(Some(prior)).unwrap().unwrap();
    let composed: serde_json::Value = serde_json::from_str(&composed).unwrap();
    let prior: serde_json::Value = serde_json::from_str(prior).unwrap();
    for key in ["permissions", "plugins", "providers"] {
        assert_eq!(composed[key], prior[key]);
    }
    assert_eq!(composed["model"], "openai/gpt-selected#high");
    assert!(
        selection
            .opencode_inline(Some("// preserve JSONC, do not partially overwrite"))
            .is_err()
    );
    let inherited = LaunchSelection::default();
    assert_eq!(
        inherited.opencode_inline(Some("// untouched")).unwrap(),
        None
    );
    let unknown = LaunchSelection {
        model: None,
        effort: Some("high".into()),
    };
    assert!(unknown.opencode_inline(None).is_err());
    let object=unknown.opencode_inline(Some(r#"{"model":{"providerID":"provider","model":"chosen","variant":"low"},"permissions":["keep"]}"#)).unwrap().unwrap();
    let object: serde_json::Value = serde_json::from_str(&object).unwrap();
    assert_eq!(
        object["model"],
        json!({"providerID":"provider","model":"chosen","variant":"high"})
    );
    assert_eq!(object["permissions"], json!(["keep"]));
    let invalid = LaunchSelection {
        model: Some("provider/model".into()),
        effort: Some("high#invalid".into()),
    };
    assert!(invalid.opencode_inline(None).is_err());
}

#[test]
fn workflow_refuses_role_selections_before_native_effect_admission() {
    let mut input = definition();
    input["roles"]["parent"]["model"] = json!("--invalid-model-option");
    let error = validate_definition(&serde_json::to_vec(&input).unwrap()).unwrap_err();
    assert!(error.contains("roles.parent.model"), "{error}");
}

#[test]
fn accepted_contract_snapshot_remains_accessible_when_original_artifacts_change() {
    use guild_session::workflow::*;
    let target = tempfile::tempdir().unwrap();
    let owned = tempfile::tempdir().unwrap();
    std::fs::write(
        target.path().join("task.md"),
        "Exact accepted scope and scenarios",
    )
    .unwrap();
    let pinned = pin_contract(target.path(), Some(std::path::Path::new("task.md")), None).unwrap();
    let snapshot = pinned.snapshot(owned.path()).unwrap();
    std::fs::write(target.path().join("task.md"), "Changed original").unwrap();
    snapshot.verify().unwrap();
    assert_eq!(pinned.snapshot(owned.path()).unwrap(), snapshot);
    assert_eq!(
        std::fs::read_to_string(snapshot.directory.join("task.md")).unwrap(),
        "Exact accepted scope and scenarios"
    );
    assert!(pinned.verify().is_err());
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        snapshot.directory.join("task.md"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    std::fs::set_permissions(&snapshot.directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(snapshot.directory.join("task.md"), "Corrupted saved copy").unwrap();
    assert!(snapshot.verify().is_err());
    assert!(
        pinned.snapshot(owned.path()).is_err(),
        "must not repair accepted bytes from changed originals"
    );
}

fn review_usage_fixture(calls: usize, closed: bool) -> Vec<u8> {
    let ids=(0..calls).map(|n|json!({"stage":match n%3 {0=>"find",1=>"synthesis",_=>"verify"},"unit":"fixture","number":n})).collect::<Vec<_>>();
    let mut events = Vec::new();
    for id in &ids {
        events.push(json!({"type":"started","attempt":id}));
        events.push(json!({"type":"finished","attempt":id,"result":{"Err":{"kind":"nonzero_exit","diagnostic":"controlled failed process"}}}));
    }
    if closed {
        events.push(json!({"type":"terminal","report":{"state":"complete","units":[{"attempts":ids.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}]}}));
    }
    events
        .into_iter()
        .map(|e| serde_json::to_string(&e).unwrap() + "\n")
        .collect::<String>()
        .into_bytes()
}

#[test]
fn imported_incomplete_usage_survives_reload_and_only_matching_growth_reconciles() {
    use guild_session::workflow::*;
    let source = digest(b"prior source");
    let contract = digest(b"prior contract");
    let mut ledger = ReviewLedger::new(
        RunId::new(),
        Limits {
            review_calls: 18,
            correction_rounds: 3,
        },
    )
    .unwrap();
    assert!(
        ledger
            .import(
                "prior",
                &source,
                &contract,
                ReviewUsage::from_events(&review_usage_fixture(1, false)).unwrap(),
                false
            )
            .is_err()
    );
    assert_eq!(
        ledger.used(),
        1,
        "an import refusal must retain observed starts"
    );
    assert_eq!(
        ledger.remaining(),
        0,
        "unfinished prior usage holds admission"
    );
    let saved = serde_json::to_vec(&ledger).unwrap();
    let mut ledger: ReviewLedger = serde_json::from_slice(&saved).unwrap();
    ledger.validate().unwrap();
    assert!(
        ledger
            .reserve("omitted-prior", &source, &contract, 1, false)
            .is_err()
    );
    assert!(
        ledger
            .import(
                "prior",
                &digest(b"other source"),
                &contract,
                ReviewUsage::from_events(&review_usage_fixture(2, true)).unwrap(),
                false
            )
            .is_err()
    );
    assert_eq!(ledger.used(), 1);
    ledger
        .import(
            "prior",
            &source,
            &contract,
            ReviewUsage::from_events(&review_usage_fixture(2, true)).unwrap(),
            false,
        )
        .unwrap();
    assert_eq!(
        (ledger.used(), ledger.reserved(), ledger.remaining()),
        (2, 0, 16)
    );
    let complete = ledger.clone();
    ledger
        .import(
            "prior",
            &source,
            &contract,
            ReviewUsage::from_events(&review_usage_fixture(2, true)).unwrap(),
            false,
        )
        .unwrap();
    assert_eq!(
        ledger, complete,
        "same completed receipt is not charged twice"
    );
    assert!(
        ledger
            .import(
                "prior",
                &source,
                &contract,
                ReviewUsage::from_events(&review_usage_fixture(1, true)).unwrap(),
                false
            )
            .is_err()
    );
    assert_eq!(
        ledger.used(),
        2,
        "contradicting history must never shrink usage"
    );
}

#[test]
fn imported_history_cannot_replace_an_unfinished_native_round_owner() {
    use guild_session::workflow::*;
    let source = digest(b"source");
    let contract = digest(b"contract");
    let mut ledger = ReviewLedger::new(
        RunId::new(),
        Limits {
            review_calls: 18,
            correction_rounds: 3,
        },
    )
    .unwrap();
    ledger
        .reserve("native", &source, &contract, 2, false)
        .unwrap();
    let before = ledger.clone();
    assert!(
        ledger
            .import_unknown("native", &source, &contract, false)
            .is_err(),
        "a prior must not take ownership of live native accounting"
    );
    assert_eq!(ledger, before);
    assert!(
        ledger
            .import(
                "native",
                &source,
                &contract,
                ReviewUsage::from_events(&review_usage_fixture(1, true)).unwrap(),
                false
            )
            .is_err()
    );
    ledger
        .observe(
            "native",
            ReviewUsage::from_events(&review_usage_fixture(1, true)).unwrap(),
        )
        .unwrap();
    assert_eq!((ledger.used(), ledger.reserved()), (1, 0));
}

#[test]
fn unreadable_imports_hold_capacity_and_outlive_older_native_snapshots() {
    use guild_session::workflow::*;
    let source = digest(b"source");
    let contract = digest(b"contract");
    let mut ledger = ReviewLedger::new(
        RunId::new(),
        Limits {
            review_calls: 2,
            correction_rounds: 1,
        },
    )
    .unwrap();
    let old = ledger.clone();
    ledger
        .import_unknown("one", &source, &contract, false)
        .unwrap();
    ledger
        .import_unknown("two", &source, &contract, false)
        .unwrap();
    assert_eq!(
        (ledger.used(), ledger.reserved(), ledger.remaining()),
        (0, 2, 0)
    );
    let mut late = old;
    late.retain_imports(&ledger).unwrap();
    assert_eq!(late, ledger);
    assert!(
        late.reserve("omission", &source, &contract, 1, false)
            .is_err()
    );
    ledger
        .import(
            "one",
            &source,
            &contract,
            ReviewUsage::from_events(&review_usage_fixture(1, true)).unwrap(),
            false,
        )
        .unwrap();
    assert_eq!((ledger.used(), ledger.reserved()), (1, 1));
    ledger
        .import(
            "two",
            &source,
            &contract,
            ReviewUsage::from_events(&review_usage_fixture(2, true)).unwrap(),
            false,
        )
        .unwrap();
    assert_eq!(
        (ledger.used(), ledger.reserved(), ledger.remaining()),
        (3, 0, 0)
    );
    ledger.validate().unwrap();
    assert!(
        ledger
            .reserve("over-cap", &source, &contract, 1, false)
            .is_err()
    );
}

#[test]
fn aggregate_review_ledger_counts_all_starts_and_never_admits_a_nineteenth() {
    use guild_session::workflow::*;
    let mut ledger = ReviewLedger::new(
        RunId::new(),
        Limits {
            review_calls: 18,
            correction_rounds: 3,
        },
    )
    .unwrap();
    let source = digest(b"actual pinned fixture source");
    let contract = digest(b"accepted fixture contract");
    let prior = ReviewUsage::from_events(&review_usage_fixture(2, true)).unwrap();
    ledger
        .import("parent-plan", &source, &contract, prior, false)
        .unwrap();
    assert_eq!(ledger.used(), 2);
    assert_eq!(ledger.remaining(), 16);
    assert!(
        ledger
            .reserve("code-01", &source, &contract, 16, false)
            .unwrap()
    );
    assert!(
        !ledger
            .reserve("code-01", &source, &contract, 16, false)
            .unwrap()
    );
    assert!(
        ledger
            .reserve("concurrent-round", &source, &contract, 1, false)
            .is_err()
    );
    ledger
        .observe(
            "code-01",
            ReviewUsage::from_events(&review_usage_fixture(16, true)).unwrap(),
        )
        .unwrap();
    assert_eq!(ledger.used(), 18);
    assert_eq!(ledger.reserved(), 0);
    assert_eq!(ledger.remaining(), 0);
    assert!(
        ledger
            .reserve("nineteenth", &source, &contract, 1, false)
            .is_err()
    );
    let saved = serde_json::to_vec(&ledger).unwrap();
    let recovered: ReviewLedger = serde_json::from_slice(&saved).unwrap();
    recovered.validate().unwrap();
    assert_eq!(recovered.used(), 18);
    assert_eq!(recovered.remaining(), 0);
}

#[test]
fn uncertain_review_usage_retains_allowance_and_corrections_share_one_ceiling() {
    use guild_session::workflow::*;
    let source = digest(b"source");
    let contract = digest(b"contract");
    let mut ledger = ReviewLedger::new(
        RunId::new(),
        Limits {
            review_calls: 18,
            correction_rounds: 3,
        },
    )
    .unwrap();
    ledger
        .reserve("plan-01", &source, &contract, 4, false)
        .unwrap();
    ledger
        .observe(
            "plan-01",
            ReviewUsage::from_events(&review_usage_fixture(1, false)).unwrap(),
        )
        .unwrap();
    assert_eq!(
        (ledger.used(), ledger.reserved(), ledger.remaining()),
        (1, 3, 14)
    );
    assert!(
        ledger
            .observe(
                "plan-01",
                ReviewUsage::from_events(&review_usage_fixture(0, true)).unwrap()
            )
            .is_err()
    );
    assert_eq!(
        (ledger.used(), ledger.reserved(), ledger.remaining()),
        (1, 3, 14)
    );
    assert!(
        ledger
            .reserve("another", &source, &contract, 1, false)
            .is_err()
    );
    ledger
        .observe(
            "plan-01",
            ReviewUsage::from_events(&review_usage_fixture(1, true)).unwrap(),
        )
        .unwrap();
    assert_eq!(
        (ledger.used(), ledger.reserved(), ledger.remaining()),
        (1, 0, 17)
    );
    for round in [
        "plan-correction",
        "code-correction",
        "code-correction-again",
    ] {
        ledger.reserve(round, &source, &contract, 2, true).unwrap();
        ledger
            .observe(
                round,
                ReviewUsage::from_events(&review_usage_fixture(1, true)).unwrap(),
            )
            .unwrap();
    }
    assert_eq!(ledger.corrections_used(), 3);
    assert!(
        ledger
            .reserve("fourth-correction", &source, &contract, 1, true)
            .is_err()
    );
    assert!(
        ledger
            .reserve("plan-01", &digest(b"changed source"), &contract, 4, false)
            .is_err()
    );
    let mut disabled = ReviewLedger::new(
        RunId::new(),
        Limits {
            review_calls: 0,
            correction_rounds: 0,
        },
    )
    .unwrap();
    assert!(
        disabled
            .reserve("disabled", &source, &contract, 1, false)
            .is_err()
    );
    assert_eq!(
        (disabled.used(), disabled.reserved(), disabled.remaining()),
        (0, 0, 0)
    );
}

#[test]
fn installed_engine_attempt_fixture_counts_a_failed_process_and_refuses_ambiguous_usage() {
    use guild_session::workflow::*;
    let bytes = include_bytes!("../fixtures/workflow-review-call-budget.jsonl");
    let usage = ReviewUsage::from_events(bytes).unwrap();
    assert_eq!(usage.calls(), 1);
    assert!(usage.closed());
    let repeated = [bytes.as_slice(), bytes.as_slice()].concat();
    assert!(ReviewUsage::from_events(&repeated).is_err());
    let original = String::from_utf8(review_usage_fixture(1, true)).unwrap();
    let mut lines = original
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    *lines
        .last_mut()
        .unwrap()
        .pointer_mut("/report/units/0/attempts/0/id/number")
        .unwrap() = json!(1);
    let mismatched = lines
        .into_iter()
        .map(|line| serde_json::to_string(&line).unwrap() + "\n")
        .collect::<String>();
    assert!(ReviewUsage::from_events(mismatched.as_bytes()).is_err());
}

#[test]
fn revust_package_pin_keeps_selected_route_and_detects_changed_launcher_helpers() {
    use guild_session::workflow::*;
    use std::os::unix::fs::PermissionsExt;
    let owned = tempfile::tempdir().unwrap();
    let package = owned.path().join("package");
    std::fs::create_dir_all(package.join("bin")).unwrap();
    std::fs::create_dir(package.join("scripts")).unwrap();
    std::fs::write(
        package.join("bin/revust"),
        "controlled executable bytes; do not execute",
    )
    .unwrap();
    for name in [
        "launch-revust.sh",
        "host-launch.py",
        "guild-launch.py",
        "guild-run.sh",
        "helper_common.py",
    ] {
        std::fs::write(
            package.join("scripts").join(name),
            "controlled launcher fixture; do not execute",
        )
        .unwrap();
    }
    for name in ["bin/revust", "scripts/launch-revust.sh"] {
        std::fs::set_permissions(package.join(name), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    std::fs::write(package.join("build.json"),serde_json::to_vec(&json!({"version":"0.1.1","binary_sha256":digest(b"controlled executable bytes; do not execute")})).unwrap()).unwrap();
    let route = owned.path().join("current");
    std::os::unix::fs::symlink(&package, &route).unwrap();
    let pin = RevustPackage::qualify(&route).unwrap();
    pin.verify().unwrap();
    assert_eq!(pin.program(), route.join("bin/revust"));
    assert_eq!(pin.launcher(), route.join("scripts/launch-revust.sh"));
    std::fs::write(
        package.join("scripts/guild-launch.py"),
        "changed host bridge",
    )
    .unwrap();
    assert!(pin.verify().is_err());
    std::fs::write(
        package.join("scripts/guild-launch.py"),
        "controlled launcher fixture; do not execute",
    )
    .unwrap();
    pin.verify().unwrap();
    std::fs::remove_file(&route).unwrap();
    std::os::unix::fs::symlink(owned.path(), &route).unwrap();
    assert!(pin.verify().is_err());
}

#[test]
#[ignore = "read-only selected installed-package qualification; explicit host path required"]
fn revust_installed_package_fixture() {
    let route = std::path::PathBuf::from(
        std::env::var_os("GUILD_TEST_REVUST_PACKAGE").expect("explicit selected package fixture"),
    );
    let package = guild_session::workflow::RevustPackage::qualify(&route).unwrap();
    package.verify().unwrap();
    println!(
        "selected route={} version={} package_sha256={}",
        package.route.display(),
        package.version,
        package.sha256
    );
    if let Some(config) = std::env::var_os("GUILD_TEST_REVUST_CONFIG") {
        let owned = tempfile::tempdir().unwrap();
        let caller = owned.path().join("caller");
        std::fs::create_dir(&caller).unwrap();
        let defaults = owned.path().join("defaults");
        package.export_defaults(&caller, &defaults).unwrap();
        let explicit_config = owned.path().join("explicit-config");
        std::fs::create_dir(&explicit_config).unwrap();
        std::fs::write(explicit_config.join("config"), "auto-exit = 7s\n").unwrap();
        let inspected = package
            .inspect_config(&caller, Some(&explicit_config), None, &caller)
            .unwrap();
        let value: Value = serde_json::from_slice(&inspected).unwrap();
        let auto = value["legacy"]["knobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["name"] == "auto-exit")
            .unwrap();
        assert_eq!(
            auto["source"], "user",
            "inspection must preserve raw setting provenance"
        );
        assert_eq!(auto["value"], "7s");
        let explicit_snapshot = guild_session::workflow::RevustConfigSnapshot::capture(
            &caller,
            &defaults,
            &inspected,
            &owned.path().join("explicit-control"),
            2,
        )
        .unwrap();
        assert!(
            std::fs::read_to_string(explicit_snapshot.config_dir().join("config"))
                .unwrap()
                .contains("auto-exit = 7s")
        );
        for (keep_open, expected) in [(false, "1s"), (true, "0s")] {
            let resolved = package
                .inspect_launcher_config(&explicit_snapshot, keep_open, &caller)
                .unwrap();
            let resolved: Value = serde_json::from_slice(&resolved).unwrap();
            let auto = resolved["legacy"]["knobs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|k| k["name"] == "auto-exit")
                .unwrap();
            assert_eq!(auto["value"], expected);
            explicit_snapshot.verify().unwrap();
        }
        let profile = std::env::var("GUILD_TEST_REVUST_PROFILE").ok();
        let before = package
            .inspect_config(
                &caller,
                Some(std::path::Path::new(&config)),
                profile.as_deref(),
                &caller,
            )
            .unwrap();
        let snapshot = guild_session::workflow::RevustConfigSnapshot::capture(
            &caller,
            &defaults,
            &before,
            &owned.path().join("control"),
            2,
        )
        .unwrap();
        let after = package
            .inspect_config(
                &snapshot.directory,
                Some(&snapshot.config_dir()),
                Some(&snapshot.profile),
                &caller,
            )
            .unwrap();
        let before: Value = serde_json::from_slice(&before).unwrap();
        let after: Value = serde_json::from_slice(&after).unwrap();
        let mut original = before["selected_profile"].clone();
        let mut frozen = after["selected_profile"].clone();
        if let Some(value) = original.as_object_mut() {
            value.remove("source");
        }
        if let Some(value) = frozen.as_object_mut() {
            value.remove("source");
        }
        assert_eq!(
            original, frozen,
            "same installed parser must resolve identical roster/runner/stages"
        );
        assert!(
            after["policy"]["values"]["budget"]["max_calls"]
                .as_u64()
                .unwrap()
                <= 2
        );
        snapshot.verify().unwrap();
        fn release(path: &std::path::Path) {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
            for entry in std::fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    release(&entry.path());
                }
            }
        }
        release(&snapshot.directory);
        release(&explicit_snapshot.directory);
        println!(
            "read-only installed config roundtrip preserved profile={} under cap2",
            snapshot.profile
        );
    }
}

#[test]
fn revust_config_snapshot_keeps_raw_winners_and_caps_allowance_without_global_writes() {
    use guild_session::workflow::*;
    let owned = tempfile::tempdir().unwrap();
    let defaults = owned.path().join("defaults");
    let user = owned.path().join("user");
    let caller = owned.path().join("caller");
    for root in [&defaults, &user, &caller] {
        std::fs::create_dir_all(root.join("prompts/profiles")).unwrap();
    }
    std::fs::write(
        defaults.join("prompts/profiles/selected.md"),
        "embedded fallback",
    )
    .unwrap();
    std::fs::write(
        user.join("prompts/profiles/selected.md"),
        "Exact authored winner {{SCOPE}}\n",
    )
    .unwrap();
    let effective = json!({"reference_revision":"revust-contract-v1","legacy":{"paths":{"config_dir":user},"knobs":[{"name":"profile","value":"selected"},{"name":"auto-exit","value":"1s"}]},
        "selected_profile":{"name":"selected","runner":{"executor":"codex","model":"preserved-model","effort":"high"}},"extension_profiles":[],
        "prompt_files":[{"layer":"user","path":"prompts/profiles/selected.md"}],
        "policy":{"values":{"budget":{"max_calls":2},"executors":{"codex":{"env":{},"options":{"sandbox":"read-only"}}},"stages":{"verify":{"enabled":false}},"retries":{"find":{"count":0}}}}});
    let snapshot = RevustConfigSnapshot::capture(
        &caller,
        &defaults,
        &serde_json::to_vec(&effective).unwrap(),
        &owned.path().join("control"),
        4,
    )
    .unwrap();
    snapshot.verify().unwrap();
    let saved: Value = serde_json::from_slice(
        &std::fs::read(snapshot.config_dir().join("extensions.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["policy"]["budget"]["max_calls"], 2);
    assert_eq!(saved["policy"]["stages"]["verify"]["enabled"], false);
    assert_eq!(saved["policy"]["retries"]["find"]["count"], 0);
    assert_eq!(
        std::fs::read_to_string(snapshot.config_dir().join("prompts/profiles/selected.md"))
            .unwrap(),
        "Exact authored winner {{SCOPE}}\n"
    );
    std::fs::write(
        user.join("prompts/profiles/selected.md"),
        "Changed original profile",
    )
    .unwrap();
    snapshot.verify().unwrap();
    assert_eq!(
        std::fs::read_to_string(user.join("prompts/profiles/selected.md")).unwrap(),
        "Changed original profile"
    );
    let mut unbounded = effective.clone();
    unbounded["policy"]["values"]["budget"]["max_calls"] = Value::Null;
    let bounded = RevustConfigSnapshot::capture(
        &caller,
        &defaults,
        &serde_json::to_vec(&unbounded).unwrap(),
        &owned.path().join("bounded"),
        1,
    )
    .unwrap();
    let saved: Value = serde_json::from_slice(
        &std::fs::read(bounded.config_dir().join("extensions.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["policy"]["budget"]["max_calls"], 1);
    let larger = bounded
        .with_allowance(&owned.path().join("larger"), 3)
        .unwrap();
    assert_eq!(
        larger.policy["budget"]["max_calls"], 3,
        "first reservation is not the original unbounded policy"
    );
    assert_eq!(larger.original_sha256, bounded.original_sha256);
    let smaller_original = snapshot
        .with_allowance(&owned.path().join("smaller-original"), 4)
        .unwrap();
    assert_eq!(smaller_original.policy["budget"]["max_calls"], 2);
    assert_eq!(
        std::fs::read_to_string(
            smaller_original
                .config_dir()
                .join("prompts/profiles/selected.md")
        )
        .unwrap(),
        "Exact authored winner {{SCOPE}}\n",
        "changed original is never reread"
    );
    assert!(
        RevustConfigSnapshot::capture(
            &caller,
            &defaults,
            &serde_json::to_vec(&effective).unwrap(),
            &owned.path().join("disabled"),
            0
        )
        .is_err()
    );
    // Snapshot directories belong only to this fixture; release their seals for cleanup.
    fn release(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                release(&entry.path());
            }
        }
    }
    release(&snapshot.directory);
    release(&bounded.directory);
    release(&larger.directory);
    release(&smaller_original.directory);
}

#[test]
#[ignore = "selected installed package preparation only; explicit controlled config required"]
fn revust_prepared_round_fixture() {
    use guild_session::workflow::*;
    let route = std::path::PathBuf::from(
        std::env::var_os("GUILD_TEST_REVUST_PACKAGE").expect("explicit selected package"),
    );
    let config = std::path::PathBuf::from(
        std::env::var_os("GUILD_TEST_REVUST_CONFIG").expect("explicit controlled configuration"),
    );
    let owned = tempfile::tempdir().unwrap();
    let caller = owned.path().join("caller");
    std::fs::create_dir(&caller).unwrap();
    std::fs::write(
        caller.join("task.md"),
        "Accepted controlled contract; no provider calls",
    )
    .unwrap();
    let package = RevustPackage::qualify(&route).unwrap();
    let defaults = owned.path().join("defaults");
    package.export_defaults(&caller, &defaults).unwrap();
    let raw = package
        .inspect_config(&caller, Some(&config), Some("codex-only"), &caller)
        .unwrap();
    let configuration =
        RevustConfigSnapshot::capture(&caller, &defaults, &raw, &owned.path().join("control"), 1)
            .unwrap();
    let contract = pin_contract(&caller, Some(std::path::Path::new("task.md")), None)
        .unwrap()
        .snapshot(&owned.path().join("contracts"))
        .unwrap();
    let source = SourceIdentity::unversioned(caller.canonicalize().unwrap(), vec![]).unwrap();
    let round = package
        .prepare_round(
            &configuration,
            RevustSubject {
                tasks_dir: owned.path().canonicalize().unwrap().join("tasks"),
                task: "workflow-fixture".into(),
                round: "plan-01".into(),
                source: source.clone(),
                contract: contract.clone(),
                scope: "Review the exact controlled contract. Literal $HOME `not shell`.".into(),
                keep_open: false,
            },
        )
        .unwrap();
    round.verify().unwrap();
    assert_eq!(
        round.archive,
        owned
            .path()
            .canonicalize()
            .unwrap()
            .join("tasks/workflow-fixture/plan-01")
    );
    assert!(round.args.iter().any(|s| s == "--auto-exit=1s"));
    assert!(round.archive.join("input/scope.md").is_file());
    assert!(
        !round.archive.join("findings.json").exists(),
        "preparation must not execute providers"
    );
    // The real selected launcher must refuse an incompatible controlled host
    // before any provider. This fixture is not a native-window observation.
    let host_bin = owned.path().join("host-bin");
    std::fs::create_dir(&host_bin).unwrap();
    let cli = host_bin.join("guild");
    std::fs::write(&cli,"#!/bin/sh\nprintf '%s\\n' '{\"answer\":\"terminal\",\"reply\":{\"kind\":\"capabilities\",\"version\":99,\"operations\":[]}}'\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let socket = owned.path().join("h.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let mut launcher = RevustLauncher::spawn(
        &round,
        &RevustHost {
            session: guild_session::SessionId::new(),
            socket,
            cli,
        },
    )
    .unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let observed = loop {
        if let Some(result) = launcher.try_wait().unwrap() {
            break result;
        }
        assert!(
            std::time::Instant::now() < until,
            "owned launcher did not exit"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert_eq!(observed.exit_code, Some(3));
    assert!(
        std::fs::read_to_string(&observed.stderr.path)
            .unwrap()
            .contains("version-1 terminal jobs with archive_viewer")
    );
    assert!(!round.archive.join("findings.json").exists());

    assert!(
        package
            .prepare_round(
                &configuration,
                RevustSubject {
                    scope: "changed scope".into(),
                    ..round.subject.clone()
                }
            )
            .is_err(),
        "existing round cannot be silently rewritten/repeated"
    );
    fn release(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        for e in std::fs::read_dir(path).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                release(&e.path());
            }
        }
    }
    release(owned.path());
}

#[test]
fn journal_primary_remains_a_complete_readable_snapshot_during_replacement() {
    use guild_session::workflow::*;
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    };
    let owned = tempfile::tempdir().unwrap();
    let path = owned.path().join("run");
    let run = RunId::new();
    let mut journal = Journal::create(&path, run).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let started = Arc::new(Barrier::new(2));
    let reading = stop.clone();
    let barrier = started.clone();
    let primary = path.join("state.json");
    let reader = std::thread::spawn(move || {
        barrier.wait();
        let mut first_error = None;
        while !reading.load(Ordering::Acquire) {
            match std::fs::read(&primary)
                .map_err(|e| e.to_string())
                .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|e| e.to_string()))
            {
                Ok(snapshot) => assert_eq!(snapshot["run"], serde_json::json!(run)),
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error)
                    }
                }
            }
        }
        first_error
    });
    started.wait();
    for index in 0..40 {
        let mutation = Mutation::new(
            &format!("replace-{index}"),
            journal.revision(),
            json!({"index":index}),
        )
        .unwrap();
        let (receipt, admitted) = journal.reserve(&mutation).unwrap();
        assert!(admitted);
        journal
            .resolve(
                receipt.attempt,
                journal.revision(),
                json!({"observed":index}),
            )
            .unwrap();
    }
    stop.store(true, Ordering::Release);
    assert_eq!(
        reader.join().unwrap(),
        None,
        "a strict reader must never need backup fallback during snapshot replacement"
    );
}

#[test]
fn installed_review_null_degradation_requires_complete_reported_sources() {
    use guild_session::workflow::*;
    let owned = tempfile::tempdir().unwrap();
    let root = owned.path().canonicalize().unwrap();
    std::fs::create_dir(root.join("revust")).unwrap();
    let outcome = json!({"state":"complete","policy_failures":[],"diagnostics":[]});
    let manifest = json!({"task":"fixture","run":"one","outcome":outcome,"finished_at":"2026-10-18T20:25:54Z","degraded":null});
    let mut report = json!({"outcome":outcome,"findings":[],"sources":{"expected":1,"reported":1,"degraded":[]}});
    std::fs::write(root.join("manifest.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("report.md"), "Controlled complete report").unwrap();
    std::fs::write(
        root.join("revust/events.jsonl"),
        "not a usage proof; bridge separately qualifies actual attempts",
    )
    .unwrap();
    let producer = guild_session::SessionId::new();
    let source = SourceIdentity::unversioned(root.clone(), vec![]).unwrap();
    let contract = digest(b"accepted fixture");
    let roots = [root.clone()];
    let context = EvidenceContext {
        producer,
        contract: &contract,
        source: &source,
        step: "plan-review",
        scenario: "review",
        surface: Surface::Review,
        expectation: Expectation::Observed,
        allowed_roots: &roots,
    };
    let check = |report: &Value| {
        std::fs::write(root.join("findings.json"), report.to_string()).unwrap();
        let artifacts = [
            "manifest.json",
            "findings.json",
            "report.md",
            "revust/events.jsonl",
        ]
        .into_iter()
        .map(|name| EvidenceArtifact {
            path: root.join(name),
            sha256: digest(&std::fs::read(root.join(name)).unwrap()),
        })
        .collect();
        Evidence {
            id: uuid::Uuid::new_v4(),
            producer,
            contract: contract.clone(),
            source: source.clone(),
            scenario: "review".into(),
            step: "plan-review".into(),
            tool: ToolObservation {
                program: "controlled-reader".into(),
                version: "1".into(),
                argv: vec!["read".into()],
            },
            observed_at_ms: 1,
            artifacts,
            observation: Observation::Review {
                task: "fixture".into(),
                round: "one".into(),
                archive: root.clone(),
            },
        }
        .verify(&context)
    };
    assert!(
        check(&report).is_ok(),
        "SDK emits null manifest degradation for an empty set; complete report sources remain required"
    );
    report["sources"]["reported"] = json!(0);
    assert!(check(&report).is_err());
    report["sources"]["reported"] = json!(1);
    report["sources"]["degraded"] = json!(["logic"]);
    assert!(check(&report).is_err());
    report["sources"]["degraded"] = Value::Null;
    assert!(check(&report).is_err());
}

#[test]
fn private_review_environment_is_recoverable_without_archiving_values() {
    use guild_session::workflow::*;
    use std::os::unix::fs::PermissionsExt;
    let owned = tempfile::tempdir().unwrap();
    let caller = owned.path().join("caller");
    let user = owned.path().join("user");
    let defaults = owned.path().join("defaults");
    for root in [&caller, &user, &defaults] {
        std::fs::create_dir(root).unwrap();
    }
    let source = user.join("extensions.json");
    let mut executors = serde_json::Map::new();
    let mut origins = serde_json::Map::new();
    let mut redacted = serde_json::Map::new();
    for executor in ["claude", "codex", "opencode"] {
        executors.insert(
            executor.into(),
            json!({"env":{"FIXTURE_ENV":format!("private-fixture-{executor}")}}),
        );
        redacted.insert(executor.into(), json!({"env":{"FIXTURE_ENV":"<redacted>"}}));
        origins.insert(
            format!("/executors/{executor}/env/FIXTURE_ENV"),
            json!({"kind":"extension","layer":"user","profile":"selected","source":source}),
        );
    }
    std::fs::write(
        &source,
        serde_json::to_vec(
            &json!({"version":1,"profiles":{"selected":{"policy":{"executors":executors}}}}),
        )
        .unwrap(),
    )
    .unwrap();
    let effective = json!({"reference_revision":"revust-contract-v1","legacy":{"paths":{"config_dir":user},"knobs":[{"name":"profile","value":"selected"}]},"selected_profile":{"name":"selected"},"prompt_files":[],"policy":{"values":{"budget":{"max_calls":null},"executors":redacted},"origins":origins}});
    let snapshot = RevustConfigSnapshot::capture(
        &caller,
        &defaults,
        &serde_json::to_vec(&effective).unwrap(),
        &owned.path().join("control"),
        2,
    )
    .unwrap();
    assert_eq!(snapshot.environment.len(), 3);
    let durable = serde_json::to_vec(&snapshot).unwrap();
    for executor in ["claude", "codex", "opencode"] {
        let secret = format!("private-fixture-{executor}");
        assert!(
            !durable
                .windows(secret.len())
                .any(|b| b == secret.as_bytes())
        );
        assert_eq!(snapshot.policy["executors"][executor]["env"], json!({}));
    }
    let lease = RevustEnvironmentLease::create(&snapshot, "plan-01")
        .unwrap()
        .unwrap();
    lease.verify().unwrap();
    let file = lease.directory.join("extensions.json");
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let private: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    for executor in ["claude", "codex", "opencode"] {
        assert_eq!(
            private["policy"]["executors"][executor]["env"]["FIXTURE_ENV"],
            format!("private-fixture-{executor}")
        );
    }
    // Dropping the live handle must retain resources for a possibly running
    // native job. A recovered value-free owner still verifies exact ownership.
    let receipt = serde_json::to_vec(&lease).unwrap();
    drop(lease);
    let recovered: RevustEnvironmentLease = serde_json::from_slice(&receipt).unwrap();
    recovered.verify().unwrap();
    let unexpected = recovered.directory.join("foreign-resource");
    std::fs::write(&unexpected, b"preserve").unwrap();
    assert!(recovered.release().is_err());
    assert!(file.exists());
    assert!(unexpected.exists());
    std::fs::remove_file(&unexpected).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(recovered.verify().is_err());
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(&file, b"{}").unwrap();
    assert!(recovered.verify().unwrap_err().contains("bytes changed"));
    recovered.release().unwrap();
    recovered.release().unwrap();
    assert!(!recovered.directory.exists());
    assert!(recovered.verify().is_err());
    std::fs::write(&source, b"changed original source").unwrap();
    let error = RevustEnvironmentLease::create(&snapshot, "code-01").unwrap_err();
    assert!(error.contains("source changed"));
    for executor in ["claude", "codex", "opencode"] {
        assert!(!error.contains(&format!("private-fixture-{executor}")));
    }
    snapshot.verify().unwrap();
    std::fs::set_permissions(
        snapshot.config_dir(),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
}

#[test]
fn corrective_work_claims_share_imported_review_limits_before_provider_start() {
    use guild_session::workflow::*;
    let source = digest(b"source");
    let contract = digest(b"contract");
    let mut ledger = ReviewLedger::new(
        RunId::new(),
        Limits {
            review_calls: 18,
            correction_rounds: 3,
        },
    )
    .unwrap();
    for id in ["parent-plan-correction", "parent-code-correction"] {
        ledger
            .import(
                id,
                &source,
                &contract,
                ReviewUsage::from_events(&review_usage_fixture(1, true)).unwrap(),
                true,
            )
            .unwrap();
    }
    assert_eq!(ledger.corrections_used(), 2);
    assert!(ledger.claim_correction("same-child-correction").unwrap());
    assert!(!ledger.claim_correction("same-child-correction").unwrap());
    assert_eq!(
        ledger.corrections_used(),
        3,
        "failed or undelivered corrective work already holds its correction"
    );
    let before = ledger.clone();
    assert!(ledger.claim_correction("fourth-correction").is_err());
    assert_eq!(ledger, before);
    assert!(
        ledger
            .reserve_for_correction(
                "code-correction-review",
                &source,
                &contract,
                2,
                "same-child-correction"
            )
            .unwrap()
    );
    assert!(
        !ledger
            .reserve_for_correction(
                "code-correction-review",
                &source,
                &contract,
                2,
                "same-child-correction"
            )
            .unwrap()
    );
    assert_eq!(
        ledger.corrections_used(),
        3,
        "review consumes its existing corrective-work claim rather than a fourth round"
    );
    ledger
        .observe(
            "code-correction-review",
            ReviewUsage::from_events(&review_usage_fixture(1, true)).unwrap(),
        )
        .unwrap();
    assert_eq!(ledger.used(), 3);
    assert_eq!(ledger.reserved(), 0);
    assert!(
        ledger
            .reserve_for_correction(
                "reuse-claim",
                &source,
                &contract,
                1,
                "same-child-correction"
            )
            .is_err()
    );
    let recovered: ReviewLedger =
        serde_json::from_slice(&serde_json::to_vec(&ledger).unwrap()).unwrap();
    recovered.validate().unwrap();
    assert_eq!(recovered, ledger);
}

#[test]
fn outcome_predicate_cannot_skip_an_unfinished_dependency_and_dispatch_its_successor() {
    use guild_session::workflow::*;
    let mut definition = Definition::shipped().unwrap();
    definition
        .steps
        .iter_mut()
        .find(|s| s.id == "plan-review")
        .unwrap()
        .condition = Some(Condition::Outcome {
        id: "contract".into(),
    });
    let graph = RunGraph::new(definition, false).unwrap();
    let nodes = graph.nodes();
    assert_eq!(
        nodes
            .iter()
            .find(|n| n.step == "plan-review")
            .unwrap()
            .state,
        NodeState::Blocked
    );
    assert_eq!(
        nodes.iter().find(|n| n.step == "build").unwrap().state,
        NodeState::Blocked
    );
}

#[test]
fn outcome_conditions_resolve_later_skipped_producers_independently_of_array_order() {
    use guild_session::workflow::*;
    let mut definition = Definition::shipped().unwrap();
    definition
        .steps
        .retain(|s| matches!(s.id.as_str(), "contract" | "plan-review"));
    definition.corrections.clear();
    definition
        .outcomes
        .retain(|id, _| matches!(id.as_str(), "contract" | "plan-review"));
    for step in &mut definition.steps {
        step.depends_on.clear();
    }
    definition.steps[0].condition = Some(Condition::Not {
        condition: Box::new(Condition::Outcome {
            id: "plan-review".into(),
        }),
    });
    definition.steps[1].condition = Some(Condition::ArchitectureOnly { value: true });
    let graph = RunGraph::new(definition.clone(), false).unwrap();
    assert_eq!(graph.nodes()[0].state, NodeState::Ready);
    assert_eq!(graph.nodes()[1].state, NodeState::Skipped);
    definition.steps.reverse();
    let reversed = RunGraph::new(definition.clone(), false).unwrap();
    assert_eq!(reversed.nodes()[1].state, NodeState::Ready);
    // A condition-only cycle has no evidence from which to infer true or false.
    definition.steps[0].condition = Some(Condition::Outcome {
        id: "contract".into(),
    });
    let cycle = RunGraph::new(definition, false).unwrap();
    assert!(cycle.nodes().iter().all(|n| n.state == NodeState::Waiting));
}

#[test]
fn omitted_checkpoint_checks_preserves_legacy_serialized_request_identity() {
    let original = serde_json::json!({"version":1,"request_id":"legacy-contract","expected_revision":2,
        "step":"contract","outcome":"contract","evidence":[]});
    let request: guild_session::workflow::CheckpointRequest =
        serde_json::from_value(original.clone()).unwrap();
    assert!(request.checks.is_none());
    assert_eq!(serde_json::to_value(request).unwrap(), original);
}
