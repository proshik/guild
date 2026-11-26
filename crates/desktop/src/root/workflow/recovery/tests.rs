//! Controlled native reopen, never a provider evaluation.
use super::*;
use gpui::AppContext;
use guild_session::{
    Desk, PermissionMode, Runs, Seat, SessionMode, SessionRecord, harness::LaunchSelection,
};

#[test]
fn isolated_exact_conversation_reopen() {
    use std::os::unix::fs::PermissionsExt;
    let owned = tempfile::tempdir().unwrap();
    let bin = owned.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let script = r##"#!/usr/bin/python3
import os,sys,json,time
if sys.argv[1:]==['--version']: print('2.1.0');sys.exit(0)
if sys.argv[1:3]==['session','export']:
 if sys.argv[3]!='ses_controlledOriginal' or '--standalone' not in sys.argv:sys.exit(4)
 print(json.dumps({'info':{'id':sys.argv[3],'directory':os.getcwd()},'messages':[]}));sys.exit(0)
p=os.environ['RECOVERY_FIXTURE']+'/launches.jsonl'
with open(p,'a') as f:f.write(json.dumps({'argv':sys.argv[1:],'session':os.environ['GUILD_SESSION'],'pid':os.getpid(),'cwd':os.getcwd(),'inline':json.loads(os.environ.get('OPENCODE_CONFIG_CONTENT','{}'))})+'\n')
time.sleep(30)
"##;
    for name in ["claude", "codex", "opencode"] {
        let p = bin.join(name);
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "root::workflow::recovery::tests::exact_conversation_fixture",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display()),
        )
        .env("HOME", owned.path())
        .env("CODEX_HOME", owned.path().join("codex"))
        .env("GUILD_STATE_DIR", owned.path().join("state"))
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .env("SHELL", "/bin/sh")
        .env("RECOVERY_FIXTURE", owned.path())
        .output()
        .unwrap();
    let witness = std::fs::read_to_string(owned.path().join("launches.jsonl")).unwrap_or_default();
    for line in witness.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let pid = v["pid"].as_i64().unwrap() as libc::pid_t;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while unsafe { libc::kill(pid, 0) } == 0 && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "owned fixture survived Root: {pid}"
        );
    }
    eprintln!("controlled native argv witnesses: {witness}");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[gpui::test]
#[ignore = "private environment owned by isolated_exact_conversation_reopen"]
fn exact_conversation_fixture(cx: &mut gpui::TestAppContext) {
    let owned = std::path::PathBuf::from(
        std::env::var_os("RECOVERY_FIXTURE").expect("isolated fixture only"),
    );
    for harness in [
        HarnessKind::Claude,
        HarnessKind::Codex,
        HarnessKind::OpenCode,
    ] {
        let folder = owned.join(format!("project-{harness:?}"));
        std::fs::create_dir(&folder).unwrap();
        let folder = folder.canonicalize().unwrap();
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, &folder);
        let seat = Seat { workspace, project };
        let runs = match harness {
            HarnessKind::Claude => Runs::Claude(SessionMode::Terminal),
            HarnessKind::Codex => Runs::Codex(SessionMode::Terminal),
            HarnessKind::OpenCode => Runs::OpenCode(SessionMode::Terminal),
        };
        let mut record = SessionRecord::new("Original fixture".into(), seat, folder.clone(), runs);
        let conversation = match harness {
            HarnessKind::Claude => record.id.to_string(),
            HarnessKind::Codex => SessionId::new().to_string(),
            HarnessKind::OpenCode => "ses_controlledOriginal".into(),
        };
        record.harness_session = Some(conversation.clone());
        record.launch_permission = Some(PermissionMode::Manual);
        record.open = false;
        record.had_turn = true;
        record.launch_selection = Some(LaunchSelection {
            model: Some("fixture/model".into()),
            effort: Some("high".into()),
        });
        let origin = record.id;
        let binding = Binding::from_record(&record, None).unwrap();
        desk.add_session(record);
        match harness {
            HarnessKind::Claude => {
                let p = crate::paths::claude_projects_dir().join("controlled");
                std::fs::create_dir_all(&p).unwrap();
                std::fs::write(p.join(format!("{conversation}.jsonl")),serde_json::json!({"type":"user","cwd":folder,"sessionId":conversation,"message":{"role":"user","content":"retained fixture history"}}).to_string()+"\n").unwrap();
            }
            HarnessKind::Codex => {
                let p = guild_session::harness::CodexHarness::home().join("sessions/2026/10/19");
                std::fs::create_dir_all(&p).unwrap();
                std::fs::write(p.join(format!("rollout-2026-10-19T00-00-00-{conversation}.jsonl")),serde_json::json!({"type":"session_meta","payload":{"id":conversation,"cwd":folder}}).to_string()+"\n").unwrap();
            }
            HarnessKind::OpenCode => {}
        }
        let entity = cx.new(|cx| {
            crate::editor::init(cx);
            Root::diagnostic(desk, owned.join(format!("state-{harness:?}")), cx)
        });
        let mut effect=entity.update(cx,|root,cx| {
            root.attach_test_hooks(&owned);root.restored_workspaces.insert(workspace);root.settings.permission_mode=PermissionMode::BypassPermissions;
            let child=root.open_tab_with(project,Runs::Codex(SessionMode::Terminal),None,false,cx).unwrap();
            root.desk.record_harness_session(child,child.to_string());
            let peer=root.workflow_binding(child).unwrap();let run=RunId::new();
            root.processes.runs.entry(run).or_default().value=Some(serde_json::json!({"origin":binding.origin,"review_ledger":{"retained":true}}));
            Reserved {startup:None,run,attempt:AttemptId::new(),revision:1,path:folder.clone(),frozen:String::new(),binding:binding.clone(),original:binding.origin.clone(),source:guild_session::workflow::SourceIdentity::unversioned(folder.clone(),vec![]).unwrap(),reconciliation:serde_json::json!({"children":[],"review_ledger":{"retained":true}}),caller:child,peer,peer_pid:root.tabs.iter().flat_map(crate::Tab::bodies).find_map(|b|match b {Body::Terminal {session,view,..} if *session==child=>view.read(cx).process_id(),_=>None}),request_epoch:root.processes.epoch,inspected:true,stopped_qualified:true,inspection:Some(Ok(()))}
        });
        effect.original.area = Some(if harness == HarnessKind::Claude {
            guild_session::AreaId::new()
        } else {
            entity.update(cx, |root, _| root.stage.areas(seat).unwrap().focused())
        });
        if harness == HarnessKind::OpenCode {
            // Actual export executable with an explicit controlled process table;
            // never let unrelated user OpenCode instances affect this fixture.
            assert!(
                inspect_origin_with(&binding, || Ok(
                    "100 1 /tmp/opencode --session ses_foreign\n".into()
                ))
                .unwrap_err()
                .contains("holder exists")
            );
            inspect_origin_with(&binding, || Ok("1 0 /sbin/launchd\n".into())).unwrap();
        } else {
            inspect_origin(&binding).unwrap();
        }
        let mut missing = binding.clone();
        missing.origin.conversation = Some("ses_absent".into());
        assert!(inspect_origin_with(&missing, || Ok(String::new())).is_err());
        entity.update(cx, |root, cx| {
            // Identity change and stale close callbacks never launch.
            let mut stale = effect.clone();
            stale.peer_pid = None;
            let mut resume_attempted = false;
            assert!(
                root.finish_workflow_recovery(&stale, &mut resume_attempted, cx)
                    .unwrap_err()
                    .contains("runtime closed or changed")
            );
            assert!(!resume_attempted, "identity refusal must not claim resume");

            root.desk.record_harness_session(origin, "changed".into());
            if harness != HarnessKind::Claude {
                assert!(
                    root.finish_workflow_recovery(&effect, &mut false, cx)
                        .unwrap_err()
                        .contains("binding/settings changed")
                );
            }
            root.desk
                .record_harness_session(origin, conversation.clone());
            root.processes
                .closed_at
                .insert(origin, effect.request_epoch + 1);
            let refused = root
                .finish_workflow_recovery(&effect, &mut false, cx)
                .unwrap_err();
            assert!(refused.contains("stale callback"), "{refused}");
            root.processes.closed_at.remove(&origin);
            let unknown = root.adopt(
                seat,
                project,
                Body::Broken {
                    session: origin,
                    reason: "unknown owner fixture".into(),
                },
                cx,
            );
            assert!(
                root.finish_workflow_recovery(&effect, &mut false, cx)
                    .unwrap_err()
                    .contains("unknown runtime ownership")
            );
            root.close_tab(unknown, cx);
            root.processes.closed_at.remove(&origin);
            let mut foreign =
                SessionRecord::new("Foreign holder".into(), seat, folder.clone(), runs);
            foreign.harness_session = Some(conversation.clone());
            let foreign = root.desk.add_session(foreign);
            assert!(
                root.finish_workflow_recovery(&effect, &mut false, cx)
                    .unwrap_err()
                    .contains("another native record")
            );
            root.desk.remove_session(foreign);
            // Child navigation may already have opened historical Process first.
            root.open_process(effect.run, false, cx).unwrap();
            let observation = root
                .finish_workflow_recovery(&effect, &mut resume_attempted, cx)
                .unwrap();
            assert!(resume_attempted);
            assert_eq!(observation["phase"], "resume_launched");
            assert_eq!(observation["resume_attempted"], true);
            assert_eq!(observation["parent_attention_required"], true);
            assert_eq!(
                observation["placement"]["fallback"],
                harness == HarnessKind::Claude
            );

            let parent = root
                .tabs
                .iter()
                .position(|t| t.sessions().any(|s| s == origin))
                .unwrap();
            assert!(
                root.tabs.get(parent + 1).is_some_and(
                    |t| matches!(t.body(),Some(Body::Process {run}) if *run==effect.run)
                ),
                "restored origin must precede its linked Process"
            );

            assert_eq!(observation["continued"], false);
            assert_eq!(observation["ready"], false);
            let other = root.desk.add_workspace("Other seat", None).unwrap();
            root.active_workspace = other;
            resume_attempted = false;
            let live = root
                .finish_workflow_recovery(&effect, &mut resume_attempted, cx)
                .unwrap();
            assert_eq!(live["phase"], "origin_live_reused");
            assert!(!resume_attempted);
            assert_eq!(live["resume_attempted"], false);
            assert_eq!(live["parent_attention_required"], false);
            assert_eq!(root.active_workspace, workspace);
        });
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let witness = loop {
            let text = std::fs::read_to_string(owned.join("launches.jsonl")).unwrap_or_default();
            let rows: Vec<serde_json::Value> = text
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .filter(|v: &serde_json::Value| v["session"] == origin.to_string())
                .collect();
            if !rows.is_empty() {
                break rows;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(witness.len(), 1, "repeat navigation launched twice");
        let argv: Vec<String> = serde_json::from_value(witness[0]["argv"].clone()).unwrap();
        let resume: Vec<String> = harness
            .impl_()
            .resume_args(&conversation)
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(argv.windows(resume.len()).any(|a| a == resume));
        if harness == HarnessKind::OpenCode {
            assert_eq!(witness[0]["inline"]["model"], "fixture/model#high");
        } else {
            assert!(argv.iter().any(|a| a == "fixture/model"));
        }
        assert!(!argv.iter().any(|a| a == "retained fixture history"));
        assert_eq!(witness[0]["cwd"], folder.to_str().unwrap());
        if harness == HarnessKind::Claude {
            assert!(
                argv.windows(2)
                    .any(|a| a == ["--permission-mode", "manual"])
            );
            assert!(argv.windows(2).any(|a| a == ["--effort", "high"]));
        }
        if harness == HarnessKind::Codex {
            assert!(argv.iter().any(|a| a == "model_reasoning_effort=\"high\""));
            assert!(!argv.iter().any(|a| a == "-s"));
        }

        // No fallback when the original identity is absent.
        effect.binding.origin.conversation = None;
        entity.update(cx, |root, cx| {
            assert!(
                root.finish_workflow_recovery(&effect, &mut false, cx)
                    .is_err()
            )
        });
        drop(entity);
    }
}

#[test]
fn original_area_or_single_same_seat_fallback_never_guesses_between_areas() {
    let mut areas = guild_session::Areas::one();
    let original = areas.focused();
    let absent = guild_session::AreaId::new();
    assert_eq!(
        recovery_area(&areas, Some(absent)).unwrap(),
        (original, true)
    );
    areas
        .split(original, guild_session::layout::Axis::Horizontal)
        .unwrap();
    assert_eq!(
        recovery_area(&areas, Some(original)).unwrap(),
        (original, false)
    );
    assert!(
        recovery_area(&areas, Some(absent))
            .unwrap_err()
            .contains("ambiguous")
    );
    assert!(recovery_area(&areas, None).is_err());
}
