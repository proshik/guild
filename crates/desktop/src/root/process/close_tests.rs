//! Exercise real close handlers, with no provider or terminal process.
use super::*;
use gpui::{AppContext, Focusable, TestAppContext};
use guild_session::{Desk, Runs, SessionMode, SessionRecord, layout::Axis};

#[gpui::test]
fn origin_close_handlers_remove_only_owned_process_panes(cx: &mut TestAppContext) {
    for route in [
        "tab",
        "keyboard",
        "menu",
        "bulk",
        "pane",
        "project",
        "duplicate",
        "mixed",
        "process-only",
        "unloaded",
        "replaced-unloaded",
        "area",
        "workspace",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut made = None;
        let mut entity = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let project = desk.open_in(workspace, dir.path());
            let seat = Seat { workspace, project };
            let origin = desk.add_session(SessionRecord::new(
                "Origin".into(),
                seat,
                dir.path().into(),
                Runs::Claude(SessionMode::Terminal),
            ));
            let other_workspace = desk.add_workspace("Survivors", None).unwrap();
            desk.open_in(other_workspace, dir.path());
            let survivors = Seat {
                workspace: other_workspace,
                project,
            };
            let child = desk.add_session(SessionRecord::new(
                "Child".into(),
                survivors,
                dir.path().into(),
                Runs::Claude(SessionMode::Terminal),
            ));
            let run = RunId::new();
            let other = RunId::new();
            let root = cx.new(|cx| {
                let mut root = Root::diagnostic(desk, dir.path().join("state"), cx);
                root.isolated_render = true;
                root.active_workspace = workspace;
                root.active_project = Some(project);
                let parent = root.adopt(
                    seat,
                    project,
                    Body::Broken {
                        session: origin,
                        reason: "free fixture".into(),
                    },
                    cx,
                );
                // Survivors in another seat also exercise global exact-run matching.
                root.adopt(
                    survivors,
                    project,
                    Body::Broken {
                        session: child,
                        reason: "free child".into(),
                    },
                    cx,
                );
                root.adopt(survivors, project, Body::Process { run: other }, cx);
                let mut panes = guild_session::Panes::one(Body::Process { run });
                let first = panes.focused();
                let sibling = panes.split(first, Axis::Horizontal).unwrap();
                panes
                    .fill(
                        sibling,
                        Body::Settings {
                            section: crate::view::settings::Section::Sessions,
                        },
                    )
                    .ok()
                    .unwrap();
                panes.focus(sibling);
                let mixed = root.adopt_panes(survivors, project, panes, cx);
                root.adopt(survivors, project, Body::Process { run }, cx);
                let binding = OriginBinding {
                    session: origin,
                    project,
                    workspace,
                    area: Some(root.tabs[0].area),
                    harness: guild_session::HarnessKind::Claude,
                    conversation: None,
                    cwd: dir.path().into(),
                };
                root.processes.runs.entry(run).or_default().value =
                    Some(serde_json::json!({"origin":binding,"review_ledger":{"retained":true}}));
                root.active_tab = Some(parent);
                made = Some((parent, mixed, run, other, child));
                root
            });
            entity = Some(root.clone());
            gpui_component::Root::new(root, window, cx)
        });
        let (parent, mixed, run, other, child) = made.unwrap();
        let root = entity.unwrap();
        visual.update(|window, cx| {
            root.update(cx, |root, cx| match route {
                "tab" => root.close_tab_asking(parent, window, cx),
                "keyboard" => root.close_active_tab(window, cx),
                "menu" => root.run_deed(
                    crate::view::menu::Deed::Tab(crate::view::menu::tabs::TabDeed::Close),
                    &crate::view::menu::Target::Tab {
                        id: parent,
                        path: None,
                        session: true,
                        index: 0,
                        of: 1,
                        neighbours: [false; 4],
                    },
                    window,
                    cx,
                ),
                "bulk" => {
                    let standalone = root
                        .tabs
                        .iter()
                        .find(|t| {
                            t.bodies().count() == 1
                                && t.bodies()
                                    .any(|b| matches!(b,Body::Process {run:r} if *r==run))
                        })
                        .unwrap()
                        .id;
                    root.close_tabs_asking(vec![parent, standalone], window, cx);
                }
                "pane" => {
                    let pane = root
                        .tabs
                        .iter()
                        .find(|t| t.id == parent)
                        .unwrap()
                        .panes
                        .focused();
                    root.close_pane_asking(parent, pane, window, cx);
                }
                "project" => root.close_project_now(root.active_project.unwrap(), cx),
                "duplicate" => {
                    let session = root
                        .tabs
                        .iter()
                        .find(|t| t.id == parent)
                        .unwrap()
                        .session()
                        .unwrap();
                    let seat = root.tabs.iter().find(|t| t.id == parent).unwrap().seat;
                    let duplicate = root.adopt(
                        seat,
                        seat.project,
                        Body::Broken {
                            session,
                            reason: "duplicate origin view".into(),
                        },
                        cx,
                    );
                    root.close_tab(parent, cx);
                    assert!(root.process_view_open(run));
                    assert!(root.desk.session(session).unwrap().open);
                    root.close_tab(duplicate, cx);
                }
                "mixed" => {
                    let tab = root.tabs.iter_mut().find(|t| t.id == parent).unwrap();
                    let origin = tab.panes.focused();
                    let unrelated = tab.panes.split(origin, Axis::Horizontal).unwrap();
                    tab.panes
                        .fill(
                            unrelated,
                            Body::Settings {
                                section: crate::view::settings::Section::Sessions,
                            },
                        )
                        .ok()
                        .unwrap();
                    tab.panes.focus(unrelated);
                    root.close_active_tab(window, cx);
                    assert!(
                        root.process_view_open(run),
                        "closing only unrelated pane preserves ownership"
                    );
                    root.close_pane_asking(parent, origin, window, cx);
                }
                "process-only" => {
                    let views: Vec<_> = root
                        .tabs
                        .iter()
                        .filter(|t| {
                            t.bodies()
                                .any(|b| matches!(b,Body::Process {run:r} if *r==run))
                        })
                        .map(|t| t.id)
                        .collect();
                    for tab in views {
                        let panes: Vec<_> = root
                            .tabs
                            .iter()
                            .find(|t| t.id == tab)
                            .unwrap()
                            .panes
                            .bodies()
                            .into_iter()
                            .filter(|(_, b)| matches!(b,Body::Process {run:r} if *r==run))
                            .map(|(p, _)| p)
                            .collect();
                        for pane in panes {
                            root.close_pane_asking(tab, pane, window, cx);
                        }
                    }
                    assert!(root.tabs.iter().any(|t| t.id == parent));
                    let reopened = root.open_process(run, true, cx).unwrap();
                    assert_eq!(root.open_process(run, true, cx), Some(reopened));
                    root.close_tab_asking(parent, window, cx);
                }
                "unloaded" => {
                    let snapshot = root
                        .processes
                        .runs
                        .get_mut(&run)
                        .unwrap()
                        .value
                        .take()
                        .unwrap();
                    root.close_tab_asking(parent, window, cx);
                    root.observe_process_reply(
                        &WorkflowReply::Snapshot {
                            run,
                            revision: 2,
                            snapshot,
                        },
                        Presentation::None,
                        cx,
                    );
                }
                "replaced-unloaded" => {
                    let snapshot = root
                        .processes
                        .runs
                        .get_mut(&run)
                        .unwrap()
                        .value
                        .take()
                        .unwrap();
                    root.close_tab_asking(parent, window, cx);
                    let targets: Vec<_> = root
                        .tabs
                        .iter()
                        .flat_map(|t| {
                            t.panes.bodies().into_iter().filter_map(move |(p, b)| {
                                matches!(b,Body::Process {run:r} if *r==run).then_some((t.id, p))
                            })
                        })
                        .collect();
                    for (t, p) in targets {
                        root.close_pane(t, p, cx);
                    }
                    let holder = root.tabs.iter().find(|t| t.id == mixed).unwrap();
                    let fresh = root.adopt(holder.seat, holder.project, Body::Process { run }, cx);
                    root.observe_process_reply(
                        &WorkflowReply::Snapshot {
                            run,
                            revision: 2,
                            snapshot,
                        },
                        Presentation::None,
                        cx,
                    );
                    assert!(
                        root.tabs.iter().any(|t| t.id == fresh),
                        "late snapshot cannot dismiss a newly opened historical view"
                    );
                    root.close_tab(fresh, cx);
                }
                "area" => {
                    let seat = root.tabs.iter().find(|t| t.id == parent).unwrap().seat;
                    let area = root.tabs.iter().find(|t| t.id == parent).unwrap().area;
                    root.stage.split(seat, area, Axis::Horizontal).unwrap();
                    root.close_area(window, cx);
                    assert!(root.tabs.iter().any(|t| t.id == parent));
                    assert!(root.process_view_open(run));
                    root.close_tab_asking(parent, window, cx);
                }
                "workspace" => root.delete_workspace_asking(root.active_workspace, window, cx),
                _ => unreachable!(),
            });
        });
        visual.run_until_parked();
        if route == "workspace" {
            assert!(visual.has_pending_prompt());
            visual.simulate_prompt_answer("Delete");
            visual.run_until_parked();
        }
        visual.update(|_, cx| {
            root.update(cx, |root, _| {
                assert!(
                    !root.process_view_open(run),
                    "{route}: every exact-run Process must close"
                );
                assert!(
                    root.process_view_open(other),
                    "{route}: unrelated run survives"
                );
                assert!(
                    root.tabs.iter().any(|t| t.sessions().any(|s| s == child)),
                    "{route}: child survives"
                );
                let mixed = root
                    .tabs
                    .iter()
                    .find(|t| t.id == mixed)
                    .expect("mixed tab survives");
                assert!(mixed.bodies().any(|b| matches!(b, Body::Settings { .. })));
                assert_eq!(
                    root.processes.runs[&run].value.as_ref().unwrap()["review_ledger"]["retained"],
                    true
                );
                assert!(
                    root.active_tab
                        .is_none_or(|id| root.tabs.iter().any(|t| t.id == id)),
                    "no dangling front after group removal"
                );
            })
        });
    }
}

#[test]
fn isolated_origin_close_confirmation() {
    let state = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["root::process::close_tests::cancelling_origin_confirmation_preserves_process_and_live_terminal","--exact","--ignored","--test-threads=1"])
        .env("GUILD_STATE_DIR",state.path()).env("GUILD_PROCESS_CLOSE_TEST","1").env("SHELL","/bin/sh")
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[gpui::test]
#[ignore = "runs in isolated_origin_close_confirmation"]
fn cancelling_origin_confirmation_preserves_process_and_live_terminal(cx: &mut TestAppContext) {
    assert_eq!(
        std::env::var("GUILD_PROCESS_CLOSE_TEST").as_deref(),
        Ok("1")
    );
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        let seat = Seat { workspace, project };
        let session = desk.add_session(SessionRecord::new(
            "Free PTY origin".into(),
            seat,
            dir.path().into(),
            Runs::Claude(SessionMode::Terminal),
        ));
        let root = cx.new(|cx| Root::diagnostic(desk, dir.path().join("state"), cx));
        let (parent, run, view) = root.update(cx, |root, cx| {
            root.isolated_render = true;
            root.active_workspace = workspace;
            root.active_project = Some(project);
            let mut command = portable_pty::CommandBuilder::new("/bin/sleep");
            command.arg("60");
            let view = crate::terminal::view::open_command(
                command,
                dir.path(),
                crate::terminal::view::Look::of(&root.settings, root.theme),
                None,
                cx,
            )
            .unwrap();
            let facts = view.read(cx).facts().clone();
            let parent = root.adopt(
                seat,
                project,
                Body::Terminal {
                    session,
                    view: view.clone(),
                    facts,
                },
                cx,
            );
            let run = RunId::new();
            let binding = OriginBinding {
                session,
                project,
                workspace,
                area: Some(root.tabs[0].area),
                harness: guild_session::HarnessKind::Claude,
                conversation: None,
                cwd: dir.path().into(),
            };
            root.processes.runs.entry(run).or_default().value =
                Some(serde_json::json!({"origin":binding}));
            root.open_process(run, false, cx);
            root.active_tab = Some(parent);
            (parent, run, view)
        });
        made = Some((root.clone(), parent, run, view));
        gpui_component::Root::new(root, window, cx)
    });
    let (root, parent, run, view) = made.unwrap();
    let previous = visual.update(|window, cx| {
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        root.update(cx, |root, cx| root.close_active_tab(window, cx));
        focus
    });
    visual.run_until_parked();
    assert!(visual.has_pending_prompt());
    // The native sheet temporarily owns the keys. Losing focus while it is up
    // must not leave a surviving terminal unable to receive input after Cancel.
    visual.update(|window, cx| window.blur(cx));
    visual.simulate_prompt_answer("Cancel");
    visual.run_until_parked();
    visual.update(|window, cx| {
        assert!(
            previous.is_focused(window),
            "Cancel restores the previous input"
        );
        root.update(cx, |root, cx| {
            assert!(root.tabs.iter().any(|t| t.id == parent));
            assert!(root.process_view_open(run));
            assert!(!view.read(cx).exited());
            assert!(
                root.desk
                    .session(
                        root.tabs
                            .iter()
                            .find(|t| t.id == parent)
                            .unwrap()
                            .session()
                            .unwrap()
                    )
                    .unwrap()
                    .open
            );
            root.close_tab_asking(parent, window, cx);
        })
    });
    visual.run_until_parked();
    visual.simulate_prompt_answer("Close");
    visual.run_until_parked();
    visual.update(|_, cx| {
        root.update(cx, |root, cx| {
            assert!(!root.tabs.iter().any(|t| t.id == parent));
            assert!(!root.process_view_open(run));
            // The retained entity intentionally separates a closed view from PTY teardown.
            // Explicitly release the controlled fixture owner before leaving the test.
            if let Some(stopped) = view.update(cx, |view, _| view.let_go()) {
                stopped
                    .recv_timeout(std::time::Duration::from_secs(3))
                    .expect("fixture PTY teardown completes");
            }
        })
    });
}

#[test]
fn isolated_forgetting_project_views() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "root::process::close_tests::forgetting_project_views_preserves_durable_workflow_files",
            "--ignored",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[gpui::test]
#[ignore = "run through isolated_forgetting_project_views"]
fn forgetting_project_views_preserves_durable_workflow_files(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut desk = Desk::default();
    desk.normalise();
    let project = desk.open_in(desk.active_workspace(), dir.path());
    let root = cx.new(|cx| {
        let mut root = Root::diagnostic(desk, dir.path().join("global-state"), cx);
        root.isolated_render = true;
        root.active_project = Some(project);
        root.settings.forget_sessions_on_close = true;
        root.adopt(
            Seat {
                workspace: root.active_workspace,
                project,
            },
            project,
            Body::Settings {
                section: crate::view::settings::Section::Sessions,
            },
            cx,
        );
        root
    });
    let retained = dir.path().join(".guild/workflow-runs/fixture/events.jsonl");
    std::fs::create_dir_all(retained.parent().unwrap()).unwrap();
    std::fs::write(&retained, b"retained workflow history\n").unwrap();
    let config = dir.path().join(".guild/workflows/development.json");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, b"retained configuration\n").unwrap();
    let previous = guild_session::ProjectState {
        tabs: vec![guild_session::OpenTab::showing(
            guild_session::Shows::Settings,
        )],
        ..Default::default()
    };
    guild_session::project_state::save(dir.path(), &previous).unwrap();
    guild_session::store::flush();
    std::fs::write(dir.path().join(".guild/COMMIT_MESSAGE"), "forgotten draft").unwrap();
    root.update(cx, |root, cx| root.close_project_now(project, cx));
    guild_session::store::flush();
    assert_eq!(
        std::fs::read(&retained).expect("project close must retain workflow history"),
        b"retained workflow history\n"
    );
    assert_eq!(std::fs::read(&config).unwrap(), b"retained configuration\n");
    std::fs::write(
        dir.path().join(".guild/state.json"),
        "corrupt primary after forgetting",
    )
    .unwrap();
    assert!(
        guild_session::project_state::load(dir.path())
            .tabs
            .is_empty(),
        "backup must not restore forgotten tabs"
    );
    assert!(!dir.path().join(".guild/COMMIT_MESSAGE").exists());
    assert!(
        guild_session::project_state::load(dir.path())
            .sessions
            .is_empty()
    );
}

#[gpui::test]
fn delayed_show_cannot_recreate_closed_origin_group_but_new_inspection_can(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut desk = Desk::default();
    desk.normalise();
    let workspace = desk.active_workspace();
    let project = desk.open_in(workspace, dir.path());
    let seat = Seat { workspace, project };
    let origin = desk.add_session(SessionRecord::new(
        "Origin".into(),
        seat,
        dir.path().into(),
        Runs::Claude(SessionMode::Terminal),
    ));
    let root = cx.new(|cx| Root::diagnostic(desk, dir.path().join("state"), cx));
    root.update(cx,|root,cx| {
        let parent=root.adopt(seat,project,Body::Broken {session:origin,reason:"controlled stopped fixture".into()},cx);
        let run=RunId::new();let snapshot=serde_json::json!({"origin":{"session":origin,"workspace":workspace,"project":project,"area":null,"harness":"claude","conversation":origin.to_string(),"cwd":dir.path()}});
        let reply=WorkflowReply::Snapshot {run,revision:1,snapshot};let before=root.processes.epoch;
        root.close_tab(parent,cx);
        for presentation in [Presentation::Select(before),Presentation::Background] {root.observe_process_reply(&reply,presentation,cx);assert!(!root.process_view_open(run));}
        root.observe_process_reply(&reply,Presentation::Select(root.processes.epoch),cx);assert!(root.process_view_open(run),"new explicit historical inspection may open evidence without a harness");
        assert!(!root.tabs.iter().any(|t|t.sessions().next().is_some()));
    });
}
