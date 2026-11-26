//! Actual native handlers under a wrapper-owned state directory, with no harness.
use super::*;
use gpui::{AppContext, TestAppContext, px};
use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord};

#[test]
fn isolated_process_selection_navigation_and_layout() {
    let owned = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "view::process::tests::process_inspection_fixture",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("GUILD_STATE_DIR", owned.path())
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .env("GUILD_PROCESS_UI_FIXTURE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[gpui::test]
#[ignore = "run through isolated_process_selection_navigation_and_layout"]
fn process_inspection_fixture(cx: &mut TestAppContext) {
    assert_eq!(
        std::env::var("GUILD_PROCESS_UI_FIXTURE").as_deref(),
        Ok("1")
    );
    cx.executor().allow_parking();
    let project_dir = tempfile::tempdir().unwrap();
    let pinned = project_dir.path().join(".guild/pinned");
    std::fs::create_dir_all(&pinned).unwrap();
    let artifact = pinned.join("note.md");
    std::fs::write(&artifact, "# Controlled UI fixture\nNo model or review.\n").unwrap();
    let mut desk = Desk::default();
    desk.normalise();
    let workspace = desk.active_workspace();
    let project = desk.open_in(workspace, project_dir.path());
    let seat = Seat { workspace, project };
    let mut record = SessionRecord::new(
        "Controlled parent".into(),
        seat,
        project_dir.path().into(),
        Runs::Claude(SessionMode::Terminal),
    );
    record.open = true;
    let parent = record.id;
    desk.add_session(record);
    let origin = OriginBinding {
        session: parent,
        workspace,
        project,
        area: None,
        harness: guild_session::HarnessKind::Claude,
        conversation: Some(parent.to_string()),
        cwd: project_dir.path().into(),
    };
    let mut definition = Definition::shipped().unwrap();
    definition.steps.reverse();
    for step in &mut definition.steps {
        step.label = format!(
            "Custom configured {} / Long configured phase label",
            step.id
        );
    }
    let graph = guild_session::workflow::RunGraph::new(definition.clone(), false).unwrap();
    let snapshot = serde_json::json!({"origin":origin,"target":project_dir.path(),"graph":{"definition":definition},"nodes":graph.nodes(),"events":[],"assignments":[],"accepted_contract":{"directory":pinned,"contract":{"artifacts":[{"path":"note.md","sha256":"fixture"}]}}});
    let ordered = process_view::phases(&snapshot).unwrap().1;
    assert!(ordered.len() > 5);
    let first = ordered[0].step.id.clone();
    let last = ordered.last().unwrap().step.id.clone();
    let run = RunId::new();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(desk, project_dir.path().join("diagnostic"), cx);
            root.isolated_render = true;
            root.adopt(
                seat,
                project,
                crate::Body::Broken {
                    session: parent,
                    reason: "Controlled native error surface; no harness".into(),
                },
                cx,
            );
            root.observe_process_reply(
                &guild_session::workflow::WorkflowReply::Snapshot {
                    run,
                    revision: 2,
                    snapshot: snapshot.clone(),
                },
                crate::root::process::Presentation::None,
                cx,
            );
            root.open_process(run, true, cx).unwrap();
            root
        });
        entity = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = entity.unwrap();
    cx.simulate_resize(gpui::size(px(1400.), px(1000.)));
    cx.run_until_parked();
    let last_selector: &'static str = Box::leak(format!("process-phase-{last}").into_boxed_str());
    let phase = cx
        .debug_bounds(last_selector)
        .expect("all configured phases have actual controls");
    cx.simulate_click(phase.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r.processes.runs[&run].selection.step.clone())
            .as_deref(),
        Some(last.as_str())
    );
    root.update(cx, |root, cx| {
        root.observe_process_reply(
            &guild_session::workflow::WorkflowReply::Snapshot {
                run,
                revision: 2,
                snapshot: snapshot.clone(),
            },
            crate::root::process::Presentation::None,
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r.processes.runs[&run].selection.step.clone())
            .as_deref(),
        Some(last.as_str())
    );
    let main = cx.debug_bounds("process-main").unwrap();
    let aside = cx.debug_bounds("process-context").unwrap();
    assert!(
        aside.left() > main.left() && aside.top() < main.bottom(),
        "wide view must have an adjacent context column"
    );
    // The full native frame enforces its own status/titlebar minimum. A
    // narrower actual pane is the supported way to exercise this body below it.
    let (process_tab, extra) = root.update(cx, |root, cx| {
        let tab = root
            .tabs
            .iter_mut()
            .find(|t| {
                t.bodies()
                    .any(|b| matches!(b,crate::Body::Process {run:r} if *r==run))
            })
            .unwrap();
        let id = tab.id;
        let pane = tab
            .panes
            .split(tab.panes.focused(), guild_session::layout::Axis::Horizontal)
            .unwrap();
        tab.panes
            .fill(pane, crate::Body::Process { run })
            .ok()
            .unwrap();
        cx.notify();
        (id, pane)
    });
    for language in [
        guild_session::Language::English,
        guild_session::Language::Russian,
    ] {
        root.update(cx, |root, cx| {
            root.settings.language = language;
            cx.notify();
        });
        cx.simulate_resize(gpui::size(px(620.), px(720.)));
        cx.run_until_parked();
        let main = cx.debug_bounds("process-main").unwrap();
        let aside = cx.debug_bounds("process-context").unwrap();
        assert!(
            aside.top() >= main.bottom(),
            "narrow context must follow main content: main={main:?}, aside={aside:?}, widths={:?}",
            root.read_with(cx, |r, _| r.processes.runs[&run]
                .viewports
                .borrow()
                .values()
                .map(|v| v.width.get())
                .collect::<Vec<_>>())
        );
        assert!(
            aside.right() <= root.read_with(cx, |root, _| root.viewport.width),
            "context cannot escape the actual native frame minimum"
        );
    }
    root.update(cx, |root, cx| root.close_pane(process_tab, extra, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(1000.)));
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r.processes.runs[&run].viewports.borrow().len()),
        1,
        "closed pane must release its geometry/scroll cache"
    );
    let first_selector: &'static str = Box::leak(format!("process-phase-{first}").into_boxed_str());
    let first_bounds = cx.debug_bounds(first_selector).unwrap();
    cx.simulate_click(first_bounds.center(), Default::default());
    cx.run_until_parked();
    // Guild component buttons intentionally preserve focus on mouse press.
    // Enter the control group from the body's ordinary keyboard focus first.
    cx.update(|window, cx| root.update(cx, |root, cx| window.focus(&root.focus, cx)));
    cx.simulate_keystrokes("tab enter");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r.processes.runs[&run].selection.step.clone())
            .as_deref(),
        Some(first.as_str())
    );
    cx.simulate_keystrokes("tab enter");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r.processes.runs[&run].selection.step.clone())
            .as_deref(),
        Some(ordered[1].step.id.as_str()),
        "phase buttons must support Tab/Enter without terminal input"
    );
    root.update(cx, |root, cx| {
        assert!(root.process_session_available(run, parent));
        root.open_process_session(run, parent, cx);
        assert_eq!(root.active_session(), Some(parent));
        root.open_process(run, true, cx);
    });
    cx.update(|window,cx|root.update(cx,|root,cx| {
        assert!(!root.process_session_available(run,SessionId::new()));
        root.open_process_file(run,&first,&artifact,window,cx);
        assert!(root.tabs.iter().flat_map(crate::Tab::bodies).any(|body|matches!(body,crate::Body::File {path,origin:crate::Origin::Project,..} if path.as_path()==std::path::Path::new(".guild/pinned/note.md"))));
        assert!(root.workflows.is_none(),"inspection must not create a dispatcher");
        assert_eq!(root.processes.runs[&run].value.as_ref(),Some(&snapshot),"selection/navigation must not mutate graph/evidence/ledger");
    }));
    guild_session::store::flush();
}

#[test]
fn draft_notice_is_distinct_from_unqualified_empty_input_and_uncertain_delivery() {
    for (reason, expected) in [
        ("draft_input", Text::ProcessHandoffDraft),
        (
            "opencode_input_unqualified",
            Text::ProcessHandoffInputUnknown,
        ),
        ("user_input", Text::ProcessHandoffInputUsed),
        ("busy_or_question", Text::ProcessHandoffBusy),
    ] {
        let mut state = serde_json::json!({"stage":"waiting","reason":reason});
        assert_eq!(super::handoff_text(&state, false), expected);
        state["stage"] = "unknown".into();
        assert_eq!(
            super::handoff_text(&state, false),
            Text::ProcessHandoffUnknown
        );
    }
}
