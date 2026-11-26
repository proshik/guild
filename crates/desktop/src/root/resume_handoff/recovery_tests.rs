//! Real CLI/socket/Root/reopen boundary; every harness executable is a local stand-in.
use super::*;
use gpui::{AppContext, TestAppContext};
use guild_session::{Desk, HarnessKind, Runs, SessionMode, SessionRecord};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const FAKE: &str = r##"#!/usr/bin/python3
import os,sys,json,time,socket,tty,select
from pathlib import Path
args=sys.argv[1:]
if args==['--version']: print('2.1.0');sys.exit(0)
base=Path(os.environ['GUILD_RECOVERY_FIXTURE'])
if '--foreign' in args:
 if os.environ.get('GUILD_FOREIGN_ROLLOUT'):
  f=open(os.environ['GUILD_FOREIGN_ROLLOUT'],'a')
  for event in ['task_started','task_complete']:f.write(json.dumps({'type':'event_msg','payload':{'type':event,'turn_id':'foreign-B'}})+'\n')
  f.flush();(base/'foreign-written').write_text('')
 while not (base/'release-foreign').exists():time.sleep(.01)
 sys.exit(0)
cwd=Path.cwd()
if args[:2]==['session','export']:
 saved=cwd/'.saved-opencode.json'
 if not saved.exists():sys.exit(3)
 value=json.loads(saved.read_text())
 if value['info']['id']!=args[2] or '--standalone' not in args:sys.exit(4)
 print(json.dumps(value))
 if (cwd/'delete-after-export').exists():saved.unlink()
 sys.exit(0)
kind=Path(sys.argv[0]).name
owner=os.environ['GUILD_SESSION'];runtime=os.environ['GUILD_RUNTIME']
if kind=='claude': conversation=args[args.index('--resume')+1]
elif kind=='opencode':conversation=args[args.index('--session')+1]
else:conversation=args[args.index('resume')+1]
with (cwd/'launches.jsonl').open('a') as f:f.write(json.dumps({'argv':args,'session':owner,'runtime':runtime,'cwd':str(cwd),'conversation':conversation})+'\n')
def post(event,turn):
 payload={'session_id':owner,'guild_session':owner,'guild_runtime':runtime,'hook_event_name':event,'turn_id':turn}
 if kind=='claude':payload['session_id']=conversation
 elif kind=='codex':payload['codex_session']=conversation
 else:payload.update(oc_session=conversation,opencode_event='prompt' if event=='UserPromptSubmit' else 'idle')
 data=json.dumps(payload).encode();s=socket.socket(socket.AF_UNIX);s.settimeout(1)
 try:
  s.connect(os.environ['GUILD_HOOK_SOCKET']);s.sendall(b'POST /hook HTTP/1.1\r\nContent-Length: '+str(len(data)).encode()+b'\r\n\r\n'+data);s.recv(4096)
 except OSError:pass
 finally:s.close()
 # Report after the hook is answered, as a harness that speaks OSC 7501.
 if event=='UserPromptSubmit':status('working')
 if event=='Stop':status('done')
def status(state):
 sys.stdout.write('\033]7501;state='+state+'\007');sys.stdout.flush()
"##;

// Kept separate from the prefix above so the fixture's transport/lifecycle is readable.
const FAKE_TAIL: &str = r##"
def disk(event,turn):
 if kind=='codex':
  with open((cwd/'rollout-path').read_text().strip(),'a') as f:f.write(json.dumps({'type':'event_msg','payload':{'type':event,'turn_id':turn}})+'\n')
  # Codex reports its turn through OSC 7501 whatever its hooks do.
  if event=='task_started':status('working')
  if event=='task_complete':status('done')
try:tty.setraw(0)
except OSError:pass
sys.stdout.write('\033[?2004h');sys.stdout.flush()
started=stopped=completed=False;buf=b'';number=0
while not (cwd/'exit').exists():
 if not started and (cwd/'begin').exists():
  started=True
  if (cwd/'tool-first').exists():post('PreToolUse','initial')
  elif (cwd/'disk-only').exists():disk('task_started','initial')
  else:disk('task_started','initial');post('UserPromptSubmit','initial')
 if started and (cwd/'tool-first').exists() and (cwd/'release-disk-start').exists():
  (cwd/'tool-first').unlink();disk('task_started','initial')
 if started and not stopped and (cwd/'stop-hook').exists():
  stopped=True;post('Stop','initial')
 if started and not completed and (cwd/'complete').exists():
  completed=True;disk('task_complete','initial')
  if not (cwd/'disk-only').exists():post('Stop','initial')
 if (cwd/'block-input').exists():
  (cwd/'input-blocked').touch();time.sleep(.01);continue
 if select.select([0],[],[],.01)[0]:
  try:data=os.read(0,65536)
  except OSError:break
  if not data:break
  buf+=data
  if buf.endswith(b'\r'):
   number+=1
   with (cwd/'input.jsonl').open('a') as f:f.write(json.dumps(buf.decode('utf-8'))+'\n')
   turn='input-'+str(number)
   if not (cwd/'delay-input-start').exists():disk('task_started',turn);post('UserPromptSubmit',turn)
   buf=b''
(cwd/'natural-exit').write_text(str(os.getpid()))
"##;

/// What a session is observed doing: a Codex session's managed-input latch
/// (its evidence), anyone else's OSC 7501 report — the fixture reports both.
fn observed(r: &crate::Root, id: SessionId) -> &'static str {
    if r.desk.session(id).and_then(|s| s.runs.harness()) == Some(guild_session::HarnessKind::Codex)
    {
        r.latch.observed_status(id)
    } else {
        r.board.observed_status(id)
    }
}
fn shown(r: &crate::Root, id: SessionId) -> guild_session::status::Status {
    if r.desk.session(id).and_then(|s| s.runs.harness()) == Some(guild_session::HarnessKind::Codex)
    {
        r.latch.status(id)
    } else {
        r.board.status(id)
    }
}

#[test]
fn isolated_public_cli_recovery_for_all_three_harnesses() {
    run_fixture("baseline");
}
#[test]
fn isolated_transport_refusal_cannot_publish_a_continuation_receipt() {
    run_fixture("transport-refusal");
}
#[test]
fn isolated_final_save_failure_live() {
    run_fixture("fault-live");
}
#[test]
fn isolated_final_save_failure_startup() {
    run_fixture("fault-startup");
}
#[test]
fn isolated_saved_workflow_attribution() {
    run_fixture("saved-workflow");
}
#[test]
fn isolated_launch_descriptor_scope() {
    run_fixture("descriptor");
}
#[test]
fn isolated_preflight_admission() {
    run_fixture("preflight");
}
#[test]
fn isolated_missing_prompt_tool_before_disk_start() {
    run_fixture("tool-first");
}
#[test]
fn isolated_stop_turn_witness_before_fresh_start() {
    run_fixture("stop-before");
}
#[test]
fn isolated_stop_turn_witness_after_fresh_completion() {
    run_fixture("stop-after");
}
#[test]
fn isolated_shared_disk_without_runtime_hook_is_unqualified() {
    run_fixture("disk-only");
}
#[test]
fn isolated_foreign_rollout_cannot_answer_local_permission() {
    run_fixture("foreign-turn");
}
#[test]
fn isolated_live_harness_limitations_name_the_actual_adapter() {
    run_fixture("live-limitations");
}
fn run_fixture(case: &str) {
    use std::os::unix::fs::PermissionsExt;
    let owned = tempfile::Builder::new()
        .prefix("gr-")
        .tempdir_in("/tmp")
        .unwrap();
    let bin = owned.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    // Only the valid prefix precedes the shared lifecycle loop.
    let prefix = FAKE;
    let scripts = owned.path().join("scripts");
    std::fs::create_dir(&scripts).unwrap();
    for name in ["claude", "codex", "opencode"] {
        let implementation = scripts.join(name);
        std::fs::write(&implementation, format!("{prefix}{FAKE_TAIL}")).unwrap();
        // A normal supported shell wrapper is the foreground group leader;
        // Python implements only this isolated no-provider harness protocol.
        let program = bin.join(name);
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\n/usr/bin/python3 '{}' \"$@\"\n",
                implementation.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // Recipes build the current CLI in this target/profile before desktop tests.
    // Bare cargo users must do the same; the explicit override supports isolated
    // target qualification without introducing a production dependency edge.
    let cli = std::env::var_os("GUILD_TEST_CLI")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("guild")
        });
    assert!(
        cli.is_file(),
        "run just test-crate guild-desktop; bare cargo requires cargo build -p guild-cli --bin guild in the same target/profile"
    );
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "root::resume_handoff::recovery_tests::public_cli_fixture",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display()),
        )
        .env("SHELL", "/bin/sh")
        .env("GUILD_RECOVERY_FIXTURE", owned.path())
        .env("GUILD_RECOVERY_CLI", cli)
        .env("GUILD_RECOVERY_CASE", case)
        .env("CLAUDE_CONFIG_DIR", owned.path().join("claude"))
        .env("CODEX_HOME", owned.path().join("codex"))
        .env("GUILD_STATE_DIR", owned.path().join("state"))
        .env("XDG_DATA_HOME", owned.path().join("data"))
        .env("XDG_CONFIG_HOME", owned.path().join("config"))
        .env("XDG_STATE_HOME", owned.path().join("xdg-state"))
        .env("XDG_CACHE_HOME", owned.path().join("cache"))
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .env("WORKFLOW_WITNESS", owned.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn tick(root: &gpui::Entity<Root>, cx: &mut TestAppContext) {
    root.update(cx, |r, cx| r.poll_status(cx));
    cx.executor().advance_clock(Duration::from_millis(20));
    cx.run_until_parked();
}
#[track_caller]
fn until(
    root: &gpui::Entity<Root>,
    cx: &mut TestAppContext,
    mut predicate: impl FnMut(&Root, &gpui::App) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        tick(root, cx);
        if root.read_with(cx, |r, cx| predicate(r, cx)) {
            break;
        }
        if Instant::now() >= deadline {
            root.read_with(cx, |r, cx| {
                for t in r.tabs.iter().flat_map(crate::Tab::bodies) {
                    if let Body::Terminal { view, .. } = t {
                        let pid = view.read(cx).process_id().unwrap_or_default();
                        let out = Command::new("/bin/ps")
                            .args(["-o", "pid=,ppid=,stat=,comm=", "-p", &pid.to_string()])
                            .output()
                            .unwrap();
                        eprintln!(
                            "owned process at timeout: {}",
                            String::from_utf8_lossy(&out.stdout)
                        );
                        eprintln!(
                            "natural-exit marker: {:?}",
                            std::fs::read_to_string(view.read(cx).cwd().join("natural-exit"))
                        );
                    }
                }
            });
        }
        assert!(Instant::now()<deadline,"fixture observation timed out: {}",root.read_with(cx,|r,cx|serde_json::to_string(&r.desk.sessions().filter(|s|s.task).map(|s|serde_json::json!({"session":s.id,"kind":s.runs.harness(),"status":observed(r, s.id),"runtime":format!("{:?}",r.continuation_runtime(s.id,cx)),"pending":r.continuation_starts.contains_key(&s.id)})).collect::<Vec<_>>()).unwrap()));
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn cli(
    root: &gpui::Entity<Root>,
    cx: &mut TestAppContext,
    base: &Path,
    controller: SessionId,
    args: &[&str],
) -> (bool, serde_json::Value) {
    let mut process = Command::new(std::env::var_os("GUILD_RECOVERY_CLI").unwrap())
        .args(args)
        .arg("--json")
        .env("GUILD_SOCKET", base.join("h.sock"))
        .env("GUILD_SESSION", controller.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        tick(root, cx);
        if process.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "public CLI did not return: {args:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = process.wait_with_output().unwrap();
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "CLI: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), value)
}
fn lines(path: PathBuf) -> usize {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .count()
}
fn post(base: &Path, payload: serde_json::Value) {
    use std::io::{Read, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(base.join("h.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let bytes = serde_json::to_vec(&payload).unwrap();
    write!(
        stream,
        "POST /hook HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        bytes.len()
    )
    .unwrap();
    stream.write_all(&bytes).unwrap();
    let mut response = [0; 128];
    let _ = stream.read(&mut response);
}

#[gpui::test]
#[ignore = "private executables, state and CLI owned by isolated_public_cli_recovery_for_all_three_harnesses"]
fn public_cli_fixture(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let base = PathBuf::from(std::env::var_os("GUILD_RECOVERY_FIXTURE").unwrap());
    let mut desk = Desk::default();
    desk.normalise();
    let home = base.join("operator");
    std::fs::create_dir(&home).unwrap();
    let project = desk.open_in(desk.active_workspace(), &home);
    let operator = desk.add_session(SessionRecord::new(
        "controller".into(),
        project,
        home,
        Runs::Shell,
    ));
    let mut tasks = Vec::new();
    for kind in [
        HarnessKind::Claude,
        HarnessKind::Codex,
        HarnessKind::OpenCode,
    ] {
        let folder = base.join(kind.program());
        std::fs::create_dir(&folder).unwrap();
        let folder = folder.canonicalize().unwrap();
        let project = desk.open_in(desk.active_workspace(), &folder);
        let runs = match kind {
            HarnessKind::Claude => Runs::Claude(SessionMode::Terminal),
            HarnessKind::Codex => Runs::Codex(SessionMode::Terminal),
            HarnessKind::OpenCode => Runs::OpenCode(SessionMode::Terminal),
        };
        let mut task = SessionRecord::new("controlled".into(), project, folder.clone(), runs);
        task.task = true;
        task.open = false;
        task.had_turn = true;
        let conversation = if kind == HarnessKind::OpenCode {
            "ses_savedExact".into()
        } else {
            task.id.to_string()
        };
        task.harness_session = Some(conversation.clone());
        match kind {
            HarnessKind::Claude => {
                let store = guild_session::transcript::projects_dir().join("fixture");
                std::fs::create_dir_all(&store).unwrap();
                std::fs::write(store.join(format!("{conversation}.jsonl")),serde_json::json!({"type":"user","cwd":folder,"sessionId":conversation,"message":{"role":"user","content":"saved"}}).to_string()+"\n").unwrap();
            }
            HarnessKind::Codex => {
                let store =
                    guild_session::harness::CodexHarness::home().join("sessions/2026/10/25");
                std::fs::create_dir_all(&store).unwrap();
                let path = store.join(format!("rollout-2026-10-25T00-00-00-{conversation}.jsonl"));
                std::fs::write(&path,serde_json::json!({"type":"session_meta","payload":{"id":conversation,"cwd":folder}}).to_string()+"\n").unwrap();
                std::fs::write(folder.join("rollout-path"), path.to_str().unwrap()).unwrap();
            }
            HarnessKind::OpenCode => {
                std::fs::write(folder.join(".saved-opencode.json"),serde_json::json!({"info":{"id":conversation,"location":{"directory":folder}},"messages":[]}).to_string()).unwrap();
            }
        }
        let id = desk.add_session(task);
        tasks.push((kind, id, conversation, folder));
    }
    let root = cx.new(|cx| {
        crate::editor::init(cx);
        Root::diagnostic(desk, base.join("state"), cx)
    });
    root.update(cx, |r, _| {
        r.attach_test_hooks(&base);
        r.restored_workspaces.insert(r.active_workspace);
    });
    let prompt = "--help\ncontinue literal $HOME; `never execute`";
    let file = base.join("continuation.txt");
    std::fs::write(&file, prompt).unwrap();
    let case = std::env::var("GUILD_RECOVERY_CASE").unwrap();
    if case == "saved-workflow" {
        let (_, id, _, folder) = &tasks[0];
        let run = guild_session::workflow::RunId::new();
        root.update(cx, |r, _| {
            r.desk.record_workflow_run(*id, run);
            let saved = base.join("saved-desk.json");
            std::fs::write(&saved, serde_json::to_vec(&r.desk).unwrap()).unwrap();
            r.desk = serde_json::from_slice(&std::fs::read(saved).unwrap()).unwrap();
            assert!(r.workflows.is_none());
        });
        // The persisted record is the sole authority left after restart.
        for missing_run in [0, 1, 2] {
            if missing_run > 0 {
                std::fs::create_dir_all(folder.join(".guild/workflow-runs")).unwrap();
            }
            if missing_run == 2 {
                std::fs::write(
                    folder.join(".guild/workflow-runs").join(run.to_string()),
                    "unreadable run directory",
                )
                .unwrap();
            }
            let (ok, result) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id.to_string(),
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    "saved-owner",
                ],
            );
            assert!(!ok, "saved workflow attribution bypassed: {result}");
            assert!(
                result["because"]
                    .as_str()
                    .unwrap()
                    .contains("workflow_owned"),
                "{result}"
            );
            assert_eq!(lines(folder.join("launches.jsonl")), 0);
            assert_eq!(lines(folder.join("input.jsonl")), 0);
            root.read_with(cx, |r, _| {
                assert!(
                    r.continuation_receipts
                        .previous(Some(operator), *id, "saved-owner")
                        .unwrap()
                        .is_none()
                )
            });
        }
        return;
    }
    if case == "preflight" {
        let (_, id, _, folder) = &tasks[0];
        for binding_gap in [false, true] {
            let old = root.update(cx, |r, _| {
                if binding_gap {
                    let mut record = r.desk.session(*id).unwrap().clone();
                    record.cwd = base.clone();
                    r.desk.remove_session(*id);
                    r.desk.add_session(record);
                    None
                } else {
                    r.hooks.take()
                }
            });
            let (send, receive) = std::sync::mpsc::channel();
            root.update(cx, |r, cx| {
                r.command_inspect_continuation(
                    &id.to_string(),
                    Some(&operator.to_string()),
                    send,
                    cx,
                )
            });
            let mut response = None;
            until(&root, cx, |_, _| {
                response = receive.try_recv().ok();
                response.is_some()
            });
            let result = serde_json::to_value(response.unwrap()).unwrap();
            assert_eq!(
                result["observation"]["readiness"], "unqualified",
                "false ready: {result}"
            );
            let reason = result["observation"]["reason"].as_str().unwrap();
            assert!(
                reason.contains(if binding_gap {
                    "worktree_binding"
                } else {
                    "provenance"
                }),
                "{result}"
            );
            let (send, receive) = std::sync::mpsc::channel();
            root.update(cx, |r, cx| {
                r.command_resume_task(
                    &id.to_string(),
                    "preflight-gap",
                    prompt,
                    Some(&operator.to_string()),
                    false,
                    send,
                    cx,
                )
            });
            assert!(matches!(receive.try_recv(), Ok(Answer::Refused { .. })));
            assert_eq!(lines(folder.join("launches.jsonl")), 0);
            root.update(cx, |r, _| {
                assert!(r.tabs.is_empty());
                assert!(
                    r.continuation_receipts
                        .previous(Some(operator), *id, "preflight-gap")
                        .unwrap()
                        .is_none()
                );
                if !binding_gap {
                    r.hooks = old;
                }
            });
        }
        return;
    }
    if case == "fault-startup" {
        let (_, id, _, folder) = &tasks[0];
        failed_outcome_excludes_send(&root, cx, &base, operator, *id, folder, prompt, false);
        return;
    }
    for (kind, id, conversation, folder) in &tasks {
        if matches!(
            case.as_str(),
            "tool-first" | "foreign-turn" | "disk-only" | "stop-before" | "stop-after"
        ) && *kind != HarnessKind::Codex
        {
            continue;
        }
        if case == "live-limitations" && *kind == HarnessKind::Codex {
            continue;
        }
        eprintln!("public recovery fixture: {kind:?}");
        let id_text = id.to_string();
        let (ok, state) = cli(
            &root,
            cx,
            &base,
            operator,
            &["session", "continuation-status", &id_text],
        );
        assert!(ok, "{state}");
        assert_eq!(
            state["observation"]["readiness"], "ready",
            "{kind:?}: {state}"
        );
        let args = [
            "session",
            "resume",
            &id_text,
            "--from-file",
            file.to_str().unwrap(),
            "--request-id",
            "startup",
        ];
        let (ok, receipt) = cli(&root, cx, &base, operator, &args);
        assert!(ok, "{kind:?}: {receipt}");
        assert_eq!(receipt["receipt"]["state"], "launched");
        if case == "descriptor" {
            let settings = root.update(cx, |r, _| {
                let saved = r.settings.clone();
                r.settings.theme_mode = Some(guild_session::settings::ThemeMode::Light);
                r.settings.window = Some(guild_session::settings::WindowPlace {
                    x: 15.,
                    y: 20.,
                    width: 900.,
                    height: 700.,
                });
                r.settings
                    .repository_settings
                    .insert(base.join("unrelated"), Default::default());
                r.settings.launch_arguments.set(
                    if *kind == HarnessKind::OpenCode {
                        HarnessKind::Claude
                    } else {
                        HarnessKind::OpenCode
                    },
                    "--unrelated".into(),
                );
                saved
            });
            let (ok, retry) = cli(&root, cx, &base, operator, &args);
            assert!(ok, "unrelated settings invalidated retry: {retry}");
            assert_eq!(receipt, retry);
            for field in [
                "arguments",
                "prefix",
                "permission",
                "codex_sandbox",
                "codex_command_approvals",
            ] {
                root.update(cx, |r, _| {
                    r.settings = settings.clone();
                    match field {
                        "arguments" => r.settings.launch_arguments.set(*kind, "--relevant".into()),
                        "prefix" => r.settings.launch_prefixes.set(*kind, "wrapper".into()),
                        "codex_sandbox" => r.settings.codex_sandbox = false,
                        "codex_command_approvals" => r.settings.codex_command_approvals = false,
                        _ => r.settings.permission_mode = guild_session::PermissionMode::Plan,
                    }
                });
                let (ok, changed) = cli(&root, cx, &base, operator, &args);
                if matches!(field, "codex_sandbox" | "codex_command_approvals")
                    && *kind != HarnessKind::Codex
                {
                    assert!(ok, "Codex setting changed another harness: {changed}");
                } else {
                    assert!(!ok, "relevant {field} change accepted: {changed}");
                }
            }
            root.update(cx, |r, _| r.settings = settings);
            std::fs::write(folder.join("exit"), "").unwrap();
            until(&root, cx, |r, cx| {
                matches!(
                    r.continuation_runtime(*id, cx),
                    Ok(Runtime::Retained { .. })
                )
            });
            continue;
        }
        let (ok, retry) = cli(&root, cx, &base, operator, &args);
        assert!(ok);
        assert_eq!(receipt, retry, "lost reply must not launch again");
        let (ok, unknown) = cli(
            &root,
            cx,
            &base,
            operator,
            &[
                "session",
                "resume",
                &id_text,
                "--from-file",
                file.to_str().unwrap(),
                "--request-id",
                "unknown-live",
            ],
        );
        assert!(!ok, "unknown live runtime accepted input: {unknown}");
        assert_eq!(lines(folder.join("input.jsonl")), 0);
        let changed = base.join("changed.txt");
        std::fs::write(&changed, "different descriptor").unwrap();
        let (ok, _) = cli(
            &root,
            cx,
            &base,
            operator,
            &[
                "session",
                "resume",
                &id_text,
                "--from-file",
                changed.to_str().unwrap(),
                "--request-id",
                "startup",
            ],
        );
        assert!(!ok, "changed descriptor reused a key");
        let (ok, _) = cli(&root, cx, &base, *id, &args);
        assert!(!ok, "a task steered another task");
        until(&root, cx, |_, _| lines(folder.join("launches.jsonl")) == 1);
        let value: serde_json::Value = serde_json::from_str(
            std::fs::read_to_string(folder.join("launches.jsonl"))
                .unwrap()
                .lines()
                .next()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["session"], id_text);
        assert_eq!(value["conversation"], *conversation);
        assert_eq!(value["cwd"], folder.to_str().unwrap());
        let argv = value["argv"].as_array().unwrap();
        assert_eq!(argv.last().unwrap(), prompt);
        assert_eq!(
            argv[argv.len() - 2],
            if *kind == HarnessKind::OpenCode {
                "--prompt"
            } else {
                "--"
            }
        );
        assert!(
            root.read_with(cx, |r, _| r.active_tab.is_none()),
            "background launch stole focus"
        );
        if matches!(case.as_str(), "stop-before" | "stop-after") {
            std::fs::write(folder.join("disk-only"), "").unwrap();
            let local = serde_json::json!({"session_id":id_text,"guild_session":id_text,
                "guild_runtime":value["runtime"],"codex_session":conversation,
                "hook_event_name":"Stop","turn_id":"initial"});
            let mut stale = local.clone();
            stale["guild_runtime"] = serde_json::json!("previous-runtime");
            post(&base, stale);
            if case == "stop-before" {
                post(&base, local.clone());
            }
            for _ in 0..5 {
                tick(&root, cx);
            }
            root.read_with(cx, |r, _| {
                assert!(
                    !r.continuation_receipts
                        .lookup(Some(operator), *id, "startup")
                        .unwrap()
                        .turn_started
                )
            });
            assert!(root.read_with(cx, |r, cx| r.continuation_live_gap(*id, cx).is_some()));
            std::fs::write(folder.join("begin"), "").unwrap();
            until(&root, cx, |r, _| observed(r, *id) == "working");
            assert!(
                root.read_with(cx, |r, cx| r.continuation_live_gap(*id, cx).is_some()),
                "Stop alone granted readiness"
            );
            std::fs::write(folder.join("complete"), "").unwrap();
            until(&root, cx, |r, _| observed(r, *id) == "done");
            if case == "stop-after" {
                post(&base, local);
            }
            until(&root, cx, |r, _| {
                r.continuation_receipts
                    .lookup(Some(operator), *id, "startup")
                    .unwrap()
                    .turn_started
            });
            until(&root, cx, |r, cx| {
                r.continuation_live_gap(*id, cx).is_none()
            });
            // The evidence qualifies, and still: while the program reports a
            // turn in progress or a wait for the person (OSC 7501), no
            // automatic input — never looser than the latch alone.
            for (record, refused) in [
                (Some(guild_session::status::Reported::Working), true),
                (
                    Some(guild_session::status::Reported::Blocked(Some(
                        guild_session::status::Kind::Permission,
                    ))),
                    true,
                ),
                (Some(guild_session::status::Reported::Blocked(None)), true),
                // Review r2: a record cleared, or a runtime that never reports,
                // is not a finished turn either.
                (None, true),
                (Some(guild_session::status::Reported::Done), false),
            ] {
                root.update(cx, |r, _| {
                    r.board.report(*id, record);
                });
                let gap = root.read_with(cx, |r, cx| r.continuation_live_gap(*id, cx));
                assert_eq!(gap.is_some(), refused, "{record:?}: {gap:?}");
                if refused {
                    assert!(gap.unwrap().contains("OSC 7501"));
                }
            }
            assert_eq!(lines(folder.join("input.jsonl")), 0);
            return;
        }
        if case == "disk-only" {
            std::fs::write(folder.join("disk-only"), "").unwrap();
            std::fs::write(folder.join("begin"), "").unwrap();
            std::fs::write(folder.join("complete"), "").unwrap();
            until(&root, cx, |r, _| observed(r, *id) == "done");
            root.read_with(cx, |r, _| {
                assert!(
                    !r.continuation_receipts
                        .lookup(Some(operator), *id, "startup")
                        .unwrap()
                        .turn_started,
                    "shared history alone acknowledged a runtime start"
                )
            });
            let (ok, result) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id_text,
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    "shared-only",
                ],
            );
            assert!(!ok, "shared history alone qualified input: {result}");
            assert!(
                result["because"]
                    .as_str()
                    .unwrap()
                    .contains("current-runtime"),
                "{result}"
            );
            assert_eq!(lines(folder.join("input.jsonl")), 0);
            return;
        }
        if case == "tool-first" {
            std::fs::write(folder.join("tool-first"), "").unwrap();
            std::fs::write(folder.join("begin"), "").unwrap();
            until(&root, cx, |r, _| observed(r, *id) == "working");
            root.read_with(cx, |r, _| {
                assert!(
                    !r.continuation_receipts
                        .lookup(Some(operator), *id, "startup")
                        .unwrap()
                        .turn_started
                )
            });
            std::fs::write(folder.join("release-disk-start"), "").unwrap();
            until(&root, cx, |r, _| {
                r.continuation_receipts
                    .lookup(Some(operator), *id, "startup")
                    .unwrap()
                    .turn_started
            });
            std::fs::write(folder.join("complete"), "").unwrap();
            until(&root, cx, |r, cx| {
                r.continuation_live_gap(*id, cx).is_none()
            });
            let (ok, answer) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id_text,
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    "after-tool-first",
                ],
            );
            assert!(
                ok,
                "fresh local start/completion did not qualify continuation: {answer}"
            );
            until(&root, cx, |_, _| lines(folder.join("input.jsonl")) == 1);
            return;
        }
        let field = if *kind == HarnessKind::Claude {
            "session_id"
        } else if *kind == HarnessKind::Codex {
            "codex_session"
        } else {
            "oc_session"
        };
        let mut stale = serde_json::json!({"session_id":id_text,"guild_session":id_text,"guild_runtime":"stale-runtime","hook_event_name":"UserPromptSubmit","turn_id":"initial","opencode_event":"prompt"});
        stale[field] = conversation.clone().into();
        post(&base, stale);
        tick(&root, cx);
        let (_, receipt) = cli(
            &root,
            cx,
            &base,
            operator,
            &[
                "session",
                "continuation-status",
                &id_text,
                "--request-id",
                "startup",
            ],
        );
        assert_ne!(
            receipt["receipt"]["turn_started"], true,
            "old runtime hook acknowledged a new process"
        );
        eprintln!("{kind:?}: exact launch confirmed; awaiting current start");
        let foreign = if *kind == HarnessKind::OpenCode {
            "ses_foreign".into()
        } else {
            SessionId::new().to_string()
        };
        let mut wrong = serde_json::json!({"session_id":id_text,"guild_session":id_text,"guild_runtime":value["runtime"],"hook_event_name":"UserPromptSubmit","turn_id":"foreign","opencode_event":"prompt"});
        wrong[field] = foreign.into();
        post(&base, wrong);
        tick(&root, cx);
        let (_, receipt) = cli(
            &root,
            cx,
            &base,
            operator,
            &[
                "session",
                "continuation-status",
                &id_text,
                "--request-id",
                "startup",
            ],
        );
        assert_ne!(
            receipt["receipt"]["turn_started"], true,
            "another conversation acknowledged this continuation"
        );
        root.update(cx, |r, _| {
            let (_, entity, epoch) = r.continuation_starts.get(id).cloned().unwrap();
            r.continuation_began(*id, entity, epoch + 1);
            assert!(
                !r.continuation_receipts
                    .lookup(Some(operator), *id, "startup")
                    .unwrap()
                    .turn_started,
                "another input epoch acknowledged this attempt"
            );
        });
        std::fs::write(folder.join("begin"), "").unwrap();
        until(&root, cx, |r, _| {
            r.continuation_receipts
                .lookup(Some(operator), *id, "startup")
                .unwrap()
                .turn_started
        });
        std::fs::write(folder.join("stop-hook"), "").unwrap();
        // A Stop hook is never independently sufficient, including for Codex.
        for _ in 0..5 {
            tick(&root, cx);
            std::thread::sleep(Duration::from_millis(5));
        }
        let (ok, blocked) = cli(
            &root,
            cx,
            &base,
            operator,
            &[
                "session",
                "resume",
                &id_text,
                "--from-file",
                file.to_str().unwrap(),
                "--request-id",
                "live",
            ],
        );
        assert!(!ok, "Stop-only admitted {kind:?}: {blocked}");
        assert_eq!(lines(folder.join("input.jsonl")), 0);
        std::fs::write(folder.join("complete"), "").unwrap();
        if *kind == HarnessKind::Codex {
            until(&root, cx, |r, cx| {
                r.continuation_live_gap(*id, cx).is_none()
            });
            if case == "foreign-turn" {
                let (ok, answer) = cli(
                    &root,
                    cx,
                    &base,
                    operator,
                    &[
                        "session",
                        "resume",
                        &id_text,
                        "--from-file",
                        file.to_str().unwrap(),
                        "--request-id",
                        "local-A",
                    ],
                );
                assert!(ok, "{answer}");
                until(&root, cx, |r, _| {
                    r.continuation_receipts
                        .lookup(Some(operator), *id, "local-A")
                        .unwrap()
                        .turn_started
                });
                post(
                    &base,
                    serde_json::json!({"session_id":id_text,"guild_session":id_text,"guild_runtime":value["runtime"],"codex_session":conversation,"hook_event_name":"PermissionRequest","turn_id":"input-1","tool_name":"Bash","tool_input":{"command":"fixture"}}),
                );
                until(&root, cx, |r, _| {
                    matches!(shown(r, *id), guild_session::status::Status::Waiting(_))
                });
                // Even an authenticated current-turn Stop is not a permission answer.
                post(
                    &base,
                    serde_json::json!({"session_id":id_text,"guild_session":id_text,
                    "guild_runtime":value["runtime"],"codex_session":conversation,
                    "hook_event_name":"Stop","turn_id":"input-1"}),
                );
                for _ in 0..5 {
                    tick(&root, cx);
                }
                root.read_with(cx, |r, _| {
                    assert!(matches!(
                        shown(r, *id),
                        guild_session::status::Status::Waiting(_)
                    ))
                });
                let mut command = Command::new("codex");
                command.args(["resume", conversation, "--foreign"]);
                command.env(
                    "GUILD_FOREIGN_ROLLOUT",
                    std::fs::read_to_string(folder.join("rollout-path")).unwrap(),
                );
                let mut foreign =
                    guild_session::process::ProcessGroup::spawn(&mut command).unwrap();
                until(&root, cx, |_, _| base.join("foreign-written").exists());
                for _ in 0..10 {
                    tick(&root, cx);
                    std::thread::sleep(Duration::from_millis(5));
                }
                let runtime = root.read_with(cx, |r, cx| r.continuation_runtime(*id, cx).unwrap());
                let holder_result = root.read_with(cx, |r, _| {
                    inspect_continuation(r.desk.session(*id).unwrap(), conversation, &runtime)
                });
                eprintln!(
                    "foreign writer: holder={holder_result:?}; status={}",
                    root.read_with(cx, |r, _| observed(r, *id))
                );
                for exited in [false, true] {
                    if exited {
                        std::fs::write(base.join("release-foreign"), "").unwrap();
                        let deadline = Instant::now() + Duration::from_secs(3);
                        while foreign.try_wait().unwrap().is_none() {
                            assert!(Instant::now() < deadline);
                            std::thread::sleep(Duration::from_millis(5));
                        }
                    }
                    let (ok, answer) = cli(
                        &root,
                        cx,
                        &base,
                        operator,
                        &[
                            "session",
                            "resume",
                            &id_text,
                            "--from-file",
                            file.to_str().unwrap(),
                            "--request-id",
                            if exited {
                                "after-foreign-exit"
                            } else {
                                "during-foreign"
                            },
                        ],
                    );
                    assert!(!ok, "external rollout answered local permission: {answer}");
                    root.read_with(cx, |r, _| {
                        assert_eq!(
                            shown(r, *id),
                            guild_session::status::Status::Waiting(
                                guild_session::status::Why::Permission
                            )
                        )
                    });
                    assert_eq!(lines(folder.join("input.jsonl")), 1);
                }
                assert!(
                    holder_result.is_err(),
                    "live inspection skipped foreign conversation holder"
                );
                return;
            }
            if case == "transport-refusal" {
                let prompt = "bounded continuation must remain unsent".to_owned();
                std::fs::write(folder.join("block-input"), "").unwrap();
                until(&root, cx, |_, _| folder.join("input-blocked").exists());
                let caller = operator.to_string();
                let before_lines = lines(folder.join("input.jsonl"));
                root.update(cx, |r, cx| {
                    let attempt = r
                        .continuation_attempt(
                            &id.to_string(),
                            "transport-refusal",
                            &prompt,
                            Some(&caller),
                            false,
                        )
                        .unwrap();
                    let runtime = r.continuation_runtime(*id, cx).unwrap();
                    let inspection = inspect_continuation(
                        r.desk.session(*id).unwrap(),
                        &attempt.conversation,
                        &runtime,
                    );
                    let PreparedContinuation::Reserved(reserved) = r
                        .reserve_checked_continuation(
                            &attempt,
                            &prompt,
                            false,
                            Some(&caller),
                            &runtime,
                            inspection,
                            cx,
                        )
                        .unwrap()
                    else {
                        panic!("fresh reservation required")
                    };
                    let view = r
                        .tabs
                        .iter()
                        .flat_map(crate::Tab::bodies)
                        .find_map(|b| {
                            if let Body::Terminal { session, view, .. } = b {
                                (*session == *id).then(|| view.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    view.update(cx, |v, cx| {
                        v.protocol_text_for_test("x".repeat(8 * 1024 * 1024), cx)
                    });
                    let epoch = view.read(cx).input_epoch();
                    let receipt = r
                        .finish_reserved_continuation(
                            reserved,
                            prompt,
                            false,
                            Some(&caller),
                            &runtime,
                            Ok(()),
                            cx,
                        )
                        .unwrap();
                    assert_eq!(
                        receipt.state,
                        resume_handoff::State::FailedToStart,
                        "rejected whole input reported submitted"
                    );
                    assert!(!receipt.turn_started);
                    assert_eq!(
                        view.read(cx).input_epoch(),
                        epoch,
                        "refusal consumed input epoch"
                    );
                    assert!(!r.continuation_starts.contains_key(id));
                    assert_eq!(
                        r.continuation_receipts
                            .lookup(Some(operator), *id, "transport-refusal")
                            .unwrap()
                            .state,
                        resume_handoff::State::FailedToStart
                    );
                });
                assert_eq!(
                    lines(folder.join("input.jsonl")),
                    before_lines,
                    "rejected input reached fake harness"
                );
                return;
            }
            if case == "fault-live" {
                std::fs::write(folder.join("delay-input-start"), "").unwrap();
                failed_outcome_excludes_send(&root, cx, &base, operator, *id, folder, prompt, true);
                return;
            }
            let (ok, receipt) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id_text,
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    "live",
                ],
            );
            assert!(ok, "{receipt}");
            assert_eq!(receipt["receipt"]["state"], "submitted");
            until(&root, cx, |_, _| lines(folder.join("input.jsonl")) == 1);
            let (ok, _) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id_text,
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    "live",
                ],
            );
            assert!(ok);
            assert_eq!(lines(folder.join("input.jsonl")), 1);
            until(&root, cx, |r, _| {
                r.continuation_receipts
                    .lookup(Some(operator), *id, "live")
                    .unwrap()
                    .turn_started
            });
        } else {
            let (_, status) = cli(
                &root,
                cx,
                &base,
                operator,
                &["session", "continuation-status", &id_text],
            );
            assert_ne!(
                status["observation"]["readiness"], "ready",
                "{kind:?}: {status}"
            );
            assert!(
                status["observation"]["reason"]
                    .as_str()
                    .unwrap()
                    .starts_with(if *kind == HarnessKind::Claude {
                        "claude_input_unqualified:"
                    } else {
                        "opencode_input_unqualified:"
                    }),
                "wrong-harness live limitation: {status}"
            );
        }
        if matches!(case.as_str(), "live-limitations" | "descriptor") {
            continue;
        }
        let original = root.read_with(cx, |r, _| {
            r.tabs
                .iter()
                .find(|t| t.sessions().any(|s| s == *id))
                .map(|t| (t.id, t.panes.ids()))
                .unwrap()
        });
        eprintln!("{kind:?}: waiting for natural retained exit");
        std::fs::write(folder.join("exit"), "").unwrap();
        until(&root, cx, |r, cx| {
            matches!(
                r.continuation_runtime(*id, cx),
                Ok(Runtime::Retained { .. })
            )
        });
        std::fs::remove_file(folder.join("exit")).unwrap();
        if *kind == HarnessKind::OpenCode {
            use std::os::unix::fs::PermissionsExt;
            let foreign_bin = base.join("foreign");
            std::fs::create_dir(&foreign_bin).unwrap();
            let program = foreign_bin.join("opencode");
            std::fs::write(&program,"#!/bin/sh\ntouch \"$GUILD_RECOVERY_FIXTURE/foreign-ready\"\nwhile [ ! -e \"$GUILD_RECOVERY_FIXTURE/release-foreign\" ]; do /bin/sleep .01; done\n").unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            let mut foreign_command = Command::new(program);
            foreign_command.args(["--session", "ses_other"]);
            let mut foreign =
                guild_session::process::ProcessGroup::spawn(&mut foreign_command).unwrap();
            until(&root, cx, |_, _| base.join("foreign-ready").exists());
            let (ok, refused) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id_text,
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    "restored",
                ],
            );
            assert!(!ok, "{refused}");
            assert!(
                refused["because"]
                    .as_str()
                    .unwrap()
                    .contains("holder exists"),
                "{refused}"
            );
            assert_eq!(lines(folder.join("launches.jsonl")), 1);
            std::fs::write(base.join("release-foreign"), "").unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while foreign.try_wait().unwrap().is_none() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            // Missing history must never fall through to OpenCode's create-if-absent flag.
            let saved = folder.join(".saved-opencode.json");
            let retained = std::fs::read(&saved).unwrap();
            std::fs::remove_file(&saved).unwrap();
            let (ok, _) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id_text,
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    "restored",
                ],
            );
            assert!(!ok);
            assert_eq!(lines(folder.join("launches.jsonl")), 1);
            std::fs::write(&saved, &retained).unwrap();
            std::fs::write(folder.join("delete-after-export"), "").unwrap();
            let (ok, answer) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id_text,
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    "history-disappeared",
                ],
            );
            assert!(
                !ok,
                "OpenCode was launched after its saved history disappeared: {answer}"
            );
            assert_eq!(lines(folder.join("launches.jsonl")), 1);
            std::fs::remove_file(folder.join("delete-after-export")).unwrap();
            std::fs::write(saved, retained).unwrap();
        }
        let (ok, receipt) = cli(
            &root,
            cx,
            &base,
            operator,
            &[
                "session",
                "resume",
                &id_text,
                "--from-file",
                file.to_str().unwrap(),
                "--request-id",
                "restored",
            ],
        );
        assert!(ok, "{kind:?}: {receipt}");
        until(&root, cx, |_, _| lines(folder.join("launches.jsonl")) == 2);
        assert_eq!(
            root.read_with(cx, |r, _| r
                .tabs
                .iter()
                .find(|t| t.sessions().any(|s| s == *id))
                .map(|t| (t.id, t.panes.ids()))
                .unwrap()),
            original,
            "retained pane changed"
        );
        assert!(root.read_with(cx, |r, _| r.active_tab.is_none()));
        // Genuine waits are observations, never input opportunities. Route through
        // the actual socket, including the current launch identity.
        until(&root, cx, |r, _| {
            r.continuation_receipts
                .lookup(Some(operator), *id, "restored")
                .unwrap()
                .turn_started
                && observed(r, *id) == "done"
        });
        let latest: serde_json::Value = serde_json::from_str(
            std::fs::read_to_string(folder.join("launches.jsonl"))
                .unwrap()
                .lines()
                .last()
                .unwrap(),
        )
        .unwrap();
        let question_turn = if *kind == HarnessKind::Codex {
            until(&root, cx, |r, cx| {
                r.continuation_live_gap(*id, cx).is_none()
            });
            let (ok, answer) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id_text,
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    "question-turn",
                ],
            );
            assert!(ok, "{answer}");
            until(&root, cx, |r, _| {
                r.continuation_receipts
                    .lookup(Some(operator), *id, "question-turn")
                    .unwrap()
                    .turn_started
            });
            "input-1"
        } else {
            "initial"
        };

        for (event, tool, oc, expected) in [
            (
                "PermissionRequest",
                "Bash",
                "permission.asked",
                "human_permission",
            ),
            (
                "PreToolUse",
                "AskUserQuestion",
                "question.asked",
                "human_question",
            ),
        ] {
            let mut payload = serde_json::json!({"guild_session":id_text,"session_id":id_text,"guild_runtime":latest["runtime"],"hook_event_name":event,"tool_name":tool,"turn_id":question_turn,"request_id":expected,"opencode_event":oc});
            let field = if *kind == HarnessKind::Claude {
                "session_id"
            } else if *kind == HarnessKind::Codex {
                "codex_session"
            } else {
                "oc_session"
            };
            payload[field] = conversation.clone().into();
            post(&base, payload);
            tick(&root, cx);
            let inputs = lines(folder.join("input.jsonl"));
            let (ok, answer) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id_text,
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    expected,
                ],
            );
            assert!(!ok, "human wait accepted input: {answer}");
            // A hook's question is evidence only for Codex's latch; a Claude
            // or OpenCode session is never qualified for automatic input.
            let because = answer["because"].as_str().unwrap();
            if *kind == HarnessKind::Codex {
                assert!(because.contains(expected), "{answer}");
            } else {
                assert!(
                    because.contains("input_unqualified") || because.contains("OSC 7501"),
                    "{answer}"
                );
            }
            assert_eq!(lines(folder.join("input.jsonl")), inputs);
        }
        if *kind == HarnessKind::Codex {
            use std::io::Write;
            let path = std::fs::read_to_string(folder.join("rollout-path")).unwrap();
            // The stand-in reports exact local tool resolutions first. Shared
            // rollout completion alone must never answer either human wait.
            for request in ["human_permission", "human_question"] {
                post(
                    &base,
                    serde_json::json!({"guild_session":id_text,"session_id":id_text,"guild_runtime":latest["runtime"],"codex_session":conversation,"hook_event_name":"PostToolUse","turn_id":question_turn,"request_id":request}),
                );
            }
            until(&root, cx, |r, _| observed(r, *id) == "working");
            // Exact completion then permits a deterministic input-epoch race;
            // no answer/input is supplied by the controller.
            writeln!(std::fs::OpenOptions::new().append(true).open(path.trim()).unwrap(),"{}",serde_json::json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":question_turn}})).unwrap();
            // …and the program reports the turn done, as Codex does (OSC 7501).
            root.update(cx, |r, _| {
                r.board
                    .report(*id, Some(guild_session::status::Reported::Done));
            });
            until(&root, cx, |r, cx| {
                r.continuation_live_gap(*id, cx).is_none()
            });
            let inputs = lines(folder.join("input.jsonl"));
            root.update(cx, |r, cx| {
                let expected = r.continuation_runtime(*id, cx).unwrap();
                let caller = operator.to_string();
                let attempt = r
                    .continuation_attempt(&id_text, "epoch-race", prompt, Some(&caller), false)
                    .unwrap();
                let view = r
                    .tabs
                    .iter()
                    .flat_map(crate::Tab::bodies)
                    .find_map(|b| match b {
                        Body::Terminal { session, view, .. } if *session == *id => {
                            Some(view.clone())
                        }
                        _ => None,
                    })
                    .unwrap();
                view.update(cx, |v, cx| {
                    let mut key = gpui::Keystroke::parse("d").unwrap();
                    key.key_char = Some("d".into());
                    v.on_key(
                        &gpui::KeyDownEvent {
                            keystroke: key,
                            is_held: false,
                            prefer_character_input: false,
                        },
                        cx,
                    );
                });
                let refused = r.finish_checked_continuation(
                    attempt,
                    prompt.into(),
                    false,
                    Some(&caller),
                    &expected,
                    Ok(()),
                    cx,
                );
                assert!(refused.unwrap_err().contains("binding_changed"));
                assert!(
                    r.continuation_receipts
                        .previous(Some(operator), *id, "epoch-race")
                        .unwrap()
                        .is_none()
                );
            });
            let (ok, _) = cli(
                &root,
                cx,
                &base,
                operator,
                &[
                    "session",
                    "resume",
                    &id_text,
                    "--from-file",
                    file.to_str().unwrap(),
                    "--request-id",
                    "draft",
                ],
            );
            assert!(!ok);
            assert_eq!(
                lines(folder.join("input.jsonl")),
                inputs,
                "managed recovery pressed Enter on a draft"
            );
        }
        // Finish only this fixture's known process naturally before the next harness.
        std::fs::write(folder.join("exit"), "").unwrap();
        until(&root, cx, |r, cx| {
            matches!(
                r.continuation_runtime(*id, cx),
                Ok(Runtime::Retained { .. })
            )
        });
    }
    if matches!(case.as_str(), "live-limitations" | "descriptor") {
        return;
    }
    let (_, id, _, folder) = &tasks[0];
    let (unvisited, workspace, before_tabs, before_workspace) = root.update(cx, |r, _| {
        let workspace = r.desk.add_workspace("Unvisited fixture", None).unwrap();
        let project = r.desk.open_in(workspace, folder);
        let mut record = r.desk.session(*id).unwrap().clone();
        record.id = SessionId::new();
        record.workspace = workspace;
        record.project = project;
        record.open = true;
        record.harness_session = Some(record.id.to_string());
        (
            r.desk.add_session(record),
            workspace,
            r.tabs.len(),
            r.active_workspace,
        )
    });
    let (ok, answer) = cli(
        &root,
        cx,
        &base,
        operator,
        &["session", "continuation-status", &unvisited.to_string()],
    );
    assert!(ok, "{answer}");
    assert_eq!(answer["observation"]["runtime"], "unknown");
    assert_ne!(answer["observation"]["readiness"], "ready");
    root.read_with(cx, |r, _| {
        assert_eq!(r.tabs.len(), before_tabs);
        assert_eq!(r.active_workspace, before_workspace);
        assert!(!r.restored_workspaces.contains(&workspace));
    });
    let duplicate = root.update(cx, |r, _| {
        let mut record = r.desk.session(*id).unwrap().clone();
        record.id = SessionId::new();
        record.task = false;
        r.desk.add_session(record)
    });
    let before = lines(folder.join("launches.jsonl"));
    let (ok, answer) = cli(
        &root,
        cx,
        &base,
        operator,
        &[
            "session",
            "resume",
            &id.to_string(),
            "--from-file",
            file.to_str().unwrap(),
            "--request-id",
            "foreign-conversation",
        ],
    );
    assert!(!ok);
    assert!(
        answer["because"]
            .as_str()
            .unwrap()
            .contains("conversation_conflict"),
        "{answer}"
    );
    assert_eq!(lines(folder.join("launches.jsonl")), before);
    root.update(cx, |r, _| r.desk.remove_session(duplicate));

    let directory = folder.join(".guild/workflow-runs");
    std::fs::create_dir_all(&directory).unwrap();
    let run = guild_session::workflow::RunId::new();
    let mut journal =
        guild_session::workflow::Journal::create(&directory.join(run.to_string()), run).unwrap();
    let mutation = guild_session::workflow::Mutation::new(
        "retained-assignment",
        0,
        serde_json::json!({"operation":"controlled_assignment"}),
    )
    .unwrap();
    let (reservation, _) = journal.reserve(&mutation).unwrap();
    journal.resolve(reservation.attempt,journal.revision(),serde_json::json!({"operation":"assignment_observed","assignment":{"state":"launch_observed","session":id,"checkout":folder,"pid":42,"bound":true,"gap":null}})).unwrap();
    drop(journal);
    let before = lines(folder.join("launches.jsonl"));
    let (ok, answer) = cli(
        &root,
        cx,
        &base,
        operator,
        &[
            "session",
            "resume",
            &id.to_string(),
            "--from-file",
            file.to_str().unwrap(),
            "--request-id",
            "workflow-bypass",
        ],
    );
    assert!(!ok, "retained workflow task bypassed original authority");
    assert!(
        answer["because"]
            .as_str()
            .unwrap()
            .contains("workflow_owned"),
        "{answer}"
    );
    assert_eq!(lines(folder.join("launches.jsonl")), before);
}

#[test]
fn live_readiness_does_not_infer_a_harness_from_a_plain_shell_owner() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = portable_pty::CommandBuilder::new("/bin/sh");
    command.args(["-c", "/bin/sleep 30"]);
    let mut terminal = crate::terminal::emulator::Emulator::spawn(
        command,
        dir.path(),
        crate::terminal::emulator::GridSize::new(80, 24),
    )
    .unwrap();
    let leader = terminal.process_id().unwrap();
    let result = super::inspect_live_runtime(leader, Some(leader), HarnessKind::Codex);
    let still_alive = unsafe { libc::kill(leader as i32, 0) } == 0;
    if let Some(done) = terminal.let_go() {
        done.recv_timeout(Duration::from_secs(3)).unwrap();
    }
    assert!(still_alive, "inspection stopped the live target");
    assert!(
        result.is_err(),
        "owned PTY is not proof of the original live harness"
    );
}

/// Fault the real Store after a durable reservation, before final outcome save.
#[allow(clippy::too_many_arguments)]
fn failed_outcome_excludes_send(
    root: &gpui::Entity<Root>,
    cx: &mut TestAppContext,
    base: &Path,
    operator: SessionId,
    id: SessionId,
    folder: &Path,
    prompt: &str,
    live: bool,
) {
    let caller = operator.to_string();
    root.update(cx, |r, cx| {
        let directory = base.join("fault-store");
        let path = directory.join("ledger.json");
        r.continuation_receipts = resume_handoff::Ledger::load(path.clone());
        let attempt = r
            .continuation_attempt(&id.to_string(), "failed-save", prompt, Some(&caller), false)
            .unwrap();
        let runtime = r.continuation_runtime(id, cx).unwrap();
        let inspection =
            inspect_continuation(r.desk.session(id).unwrap(), &attempt.conversation, &runtime);
        let PreparedContinuation::Reserved(reserved) = r
            .reserve_checked_continuation(
                &attempt,
                prompt,
                false,
                Some(&caller),
                &runtime,
                inspection,
                cx,
            )
            .unwrap()
        else {
            panic!("fresh reservation required")
        };
        std::fs::rename(&directory, base.join("reserved-store")).unwrap();
        std::fs::write(&directory, "block parent directory creation").unwrap();
        let result = r.finish_reserved_continuation(
            reserved,
            prompt.into(),
            false,
            Some(&caller),
            &runtime,
            Ok(()),
            cx,
        );
        assert!(result.is_err(), "save failure must never report success");
        let (send, receive) = std::sync::mpsc::channel();
        r.submit_agent_prompt(
            id,
            "second bytes",
            super::super::agent_submit::Origin::Command,
            send,
            cx,
        );
        // Exclusion must work immediately, before delayed/missing start telemetry.
        let answer = receive.try_recv();
        assert!(
            matches!(answer, Ok(Answer::Refused { .. })),
            "ordinary send was admitted after effect/storage failure: {answer:?}"
        );
        assert!(r.continuation_starts.contains_key(&id));
        assert_eq!(observed(r, id), "unknown");
        let (_, entity, epoch) = r.continuation_starts.get(&id).cloned().unwrap();
        r.continuation_began(id, entity, epoch);
        assert!(
            r.continuation_starts.contains_key(&id),
            "non-durable start cannot release exclusion"
        );
    });
    until(root, cx, |_, _| {
        lines(folder.join(if live {
            "input.jsonl"
        } else {
            "launches.jsonl"
        })) == 1
    });
    for _ in 0..10 {
        tick(root, cx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(lines(folder.join("input.jsonl")), usize::from(live));
    assert_eq!(lines(folder.join("launches.jsonl")), 1);
    root.read_with(cx, |r, _| assert!(r.continuation_starts.contains_key(&id)));
}
