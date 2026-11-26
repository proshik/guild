//! Real native movement entry points; no process is launched by this fixture.
use super::*;
use gpui::{AppContext, TestAppContext};
use guild_session::{Desk, Runs, SessionMode, SessionRecord, layout::Axis};
#[test]
fn isolated_process_group_movement() {
    let state = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","root::process::move_tests::moving_either_process_member_preserves_group_and_unrelated_panes","--ignored","--nocapture"]).env("GUILD_STATE_DIR",state.path()).env("GUILD_NO_GLOBAL_HOOKS","1").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[gpui::test]
#[ignore = "run through isolated_process_group_movement"]
fn moving_either_process_member_preserves_group_and_unrelated_panes(cx: &mut TestAppContext) {
    for route in [
        "closed-restored",
        "closed-stale-restored",
        "origin",
        "process",
        "drop",
        "mixed-origin",
        "mixed-process",
        "drop-after",
        "member-already-there",
        "unloaded",
        "legacy-unloaded",
        "legacy-partial",
        "orphan-unrelated",
        "foreign-target",
        "foreign-member",
        "foreign-only-members",
        "drop-self",
        "menu",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut entity = None;
        let mut ids = None;
        let (_,visual)=cx.add_window_view(|window,cx|{
            crate::editor::init(cx);let mut desk=Desk::default();desk.normalise();let workspace=desk.active_workspace();let project=desk.open_in(workspace,dir.path());let seat=Seat{workspace,project};
            let origin=desk.add_session(SessionRecord::new("Origin".into(),seat,dir.path().into(),Runs::Claude(SessionMode::Terminal)));
            let child=desk.add_session(SessionRecord::new("Child".into(),seat,dir.path().into(),Runs::Claude(SessionMode::Terminal)));
            let run=RunId::new();let unrelated=RunId::new();
            let root=cx.new(|cx|{
                let mut r=Root::diagnostic(desk,dir.path().join("state"),cx);r.isolated_render=true;r.active_workspace=workspace;r.active_project=Some(project);
                let parent=r.adopt(seat,project,Body::Broken{session:origin,reason:"retained native fixture".into()},cx);
                let process=r.adopt(seat,project,Body::Process{run},cx);let duplicate=r.adopt(seat,project,Body::Process{run},cx);
                let childtab=r.adopt(seat,project,Body::Broken{session:child,reason:"unrelated child".into()},cx);let other=r.adopt(seat,project,Body::Process{run:unrelated},cx);
                let from=r.tabs[0].area;let to=r.stage.split(seat,from,Axis::Horizontal).unwrap();
                // A second Settings view would violate its shared single-surface owner.
                // Use an ordinary recorded, unstarted session as the unrelated area anchor.
                let target_session=r.desk.add_session(SessionRecord::new("Area target".into(),seat,dir.path().into(),Runs::Claude(SessionMode::Terminal)));
                let target=r.adopt(seat,project,Body::Broken{session:target_session,reason:"controlled area anchor; no process".into()},cx);r.tabs.iter_mut().find(|t|t.id==target).unwrap().area=to;
                r.observe_process_reply(&WorkflowReply::Snapshot {run,revision:1,snapshot:serde_json::json!({"origin":{"session":origin},"review_ledger":{"unchanged":true}})},Presentation::None,cx);
                if route=="unloaded" {
                    guild_session::store::flush();
                    assert_eq!(guild_session::project_state::load(dir.path()).process_origins.get(&run),Some(&origin),"asynchronously discovered association must survive without another layout gesture");
                }
                let moved=if route=="process"||route=="mixed-process"{process}else{parent};
                let mut kept=None;
                if route.starts_with("mixed") {
                    let t=r.tabs.iter_mut().find(|t|t.id==moved).unwrap();let first=t.panes.focused();let sibling=t.panes.split(first,Axis::Horizontal).unwrap();t.panes.fill(sibling,Body::Settings{section:crate::view::settings::Section::General}).ok().unwrap();t.panes.focus(sibling);kept=Some((moved,sibling,first));
                }
                r.processes.runs.entry(unrelated).or_default().value=Some(serde_json::json!({"origin":{"session":child}}));
                r.active_tab=Some(moved);ids=Some((parent,process,duplicate,childtab,other,target,origin,run,from,to,kept));r
            });entity=Some(root.clone());gpui_component::Root::new(root,window,cx)
        });
        let (parent, process, duplicate, child, other, target, origin, run, from, to, kept) =
            ids.unwrap();
        let root = entity.unwrap();
        visual.update(|window, cx| {
            root.update(cx, |r, cx| {
                let moved = if route == "process" || route == "mixed-process" {
                    process
                } else {
                    parent
                };
                if route.starts_with("closed-") {
                    let seat = r.tabs.iter().find(|t| t.id == parent).unwrap().seat;
                    let binding = guild_session::workflow::OriginBinding {
                        session: origin,
                        project: seat.project,
                        workspace: seat.workspace,
                        area: Some(from),
                        harness: guild_session::HarnessKind::Claude,
                        conversation: Some(origin.to_string()),
                        cwd: dir.path().into(),
                    };
                    r.observe_process_reply(
                        &WorkflowReply::Snapshot {
                            run,
                            revision: 1,
                            snapshot: serde_json::json!({"origin": binding}),
                        },
                        Presentation::None,
                        cx,
                    );
                    assert_eq!(r.open_process(run, true, cx), Some(process));
                    // Close actual views, save, and reload with no cached snapshots.
                    // An old file may still contain a now-closed association.
                    r.close_tab(process, cx);
                    r.close_tab(duplicate, cx);
                    r.processes.runs.get_mut(&run).unwrap().value = None;
                    assert!(
                        r.process_move_group(parent).unwrap().is_none(),
                        "a closed cached hint is not a movement member"
                    );
                    r.store_tabs();
                    guild_session::store::flush();
                    let mut saved = guild_session::project_state::load(dir.path());
                    if route == "closed-restored" {
                        assert!(
                            !saved.process_origins.contains_key(&run),
                            "closed views must not accumulate placement metadata"
                        );
                    }
                    if route == "closed-stale-restored" {
                        saved.process_origins.insert(run, origin);
                    }
                    for seat in saved.seats.values_mut() {
                        for strip in seat.areas.as_mut().unwrap().strips.values_mut() {
                            strip
                                .tabs
                                .retain(|t| !matches!(t.shows, guild_session::Shows::Session(_)));
                            strip.active = None;
                        }
                    }
                    guild_session::project_state::save(dir.path(), &saved).unwrap();
                    guild_session::store::flush();
                    r.tabs.retain(|t| t.sessions().next().is_some());
                    r.processes = Default::default();
                    r.restored_workspaces.remove(&r.active_workspace);
                    r.restore_tabs(cx);
                    assert!(
                        !r.processes.runs.contains_key(&run),
                        "closed metadata must not restore an unloaded view"
                    );
                    r.move_tab_to_area(parent, to, None, window, cx);
                    assert_eq!(r.tabs.iter().find(|t| t.id == parent).unwrap().area, to);
                    r.reorder_tab(parent, child, false, window, cx);
                    assert_eq!(r.tabs.iter().find(|t| t.id == parent).unwrap().area, from);
                    assert!(
                        r.tabs.iter().position(|t| t.id == parent).unwrap()
                            > r.tabs.iter().position(|t| t.id == child).unwrap()
                    );
                    return;
                }
                if route == "orphan-unrelated" {
                    r.processes.runs.get_mut(&run).unwrap().value = None;
                    // The child here is an ordinary unrelated user session, not an assigned task.
                    r.processes.runs.clear();
                    r.move_tab_to_area(child, to, None, window, cx);
                    assert_eq!(
                        r.tabs.iter().find(|t| t.id == child).unwrap().area,
                        to,
                        "an orphan Process must not block an unrelated ordinary session"
                    );
                    assert_eq!(r.tabs.iter().find(|t| t.id == parent).unwrap().area, from);
                    return;
                }
                if route == "member-already-there" {
                    r.tabs.iter_mut().find(|t| t.id == moved).unwrap().area = to;
                }
                if matches!(
                    route,
                    "unloaded"
                        | "legacy-unloaded"
                        | "legacy-partial"
                        | "foreign-target"
                        | "foreign-member"
                        | "foreign-only-members"
                        | "drop-self"
                ) {
                    if matches!(route, "unloaded" | "legacy-unloaded" | "legacy-partial") {
                        // Save through the real writer, then restore only view bodies
                        // with default unloaded snapshots. No injected origin hint.
                        r.store_tabs();
                        guild_session::store::flush();
                        let mut saved = guild_session::project_state::load(dir.path());
                        if route.starts_with("legacy-") {
                            saved.process_origins.clear();
                        }
                        for seat in saved.seats.values_mut() {
                            if let Some(areas) = seat.areas.as_mut() {
                                for strip in areas.strips.values_mut() {
                                    strip.tabs.retain(|t| {
                                        !matches!(t.shows, guild_session::Shows::Session(_))
                                    });
                                    strip.active = None;
                                }
                            }
                        }
                        guild_session::project_state::save(dir.path(), &saved).unwrap();
                        guild_session::store::flush();
                        r.tabs.retain(|t| t.sessions().next().is_some());
                        r.processes = Default::default();
                        r.restored_workspaces.remove(&r.active_workspace);
                        r.restore_tabs(cx);
                        assert!(r.processes.runs.get(&run).is_none_or(|s| s.value.is_none()));
                        if route == "legacy-partial" {
                            r.observe_process_reply(
                                &WorkflowReply::Snapshot {
                                    run,
                                    revision: 1,
                                    snapshot: serde_json::json!({"origin":{"session":origin}}),
                                },
                                Presentation::None,
                                cx,
                            );
                        }
                    }
                    if matches!(route, "foreign-member" | "foreign-only-members") {
                        let workspace = r.desk.add_workspace("Foreign", None).unwrap();
                        r.tabs
                            .iter_mut()
                            .find(|t| t.id == duplicate)
                            .unwrap()
                            .seat
                            .workspace = workspace;
                        if route == "foreign-only-members" {
                            r.tabs
                                .iter_mut()
                                .find(|t| t.id == process)
                                .unwrap()
                                .seat
                                .workspace = workspace;
                        }
                    }
                    let before: Vec<_> = r
                        .tabs
                        .iter()
                        .map(|t| (t.id, t.seat, t.area, t.panes.ids()))
                        .collect();
                    if route == "drop-self" {
                        r.reorder_tab(parent, process, true, window, cx)
                    } else {
                        r.move_tab_to_area(
                            moved,
                            if route == "foreign-target" {
                                guild_session::AreaId::new()
                            } else {
                                to
                            },
                            None,
                            window,
                            cx,
                        );
                    }
                    assert_eq!(
                        r.tabs
                            .iter()
                            .map(|t| (t.id, t.seat, t.area, t.panes.ids()))
                            .collect::<Vec<_>>(),
                        before,
                        "{route}: refused movement must be atomic"
                    );
                    if route.starts_with("legacy-") {
                        let pending: Vec<_> = r
                            .tabs
                            .iter()
                            .flat_map(crate::Tab::bodies)
                            .filter_map(|b| match b {
                                Body::Process { run } => Some(*run),
                                _ => None,
                            })
                            .collect();
                        for run in pending {
                            r.observe_process_reply(
                                &WorkflowReply::Gap {
                                    code: "workflow".into(),
                                    message: "Controlled missing historical run".into(),
                                    run: Some(run),
                                    revision: None,
                                },
                                Presentation::None,
                                cx,
                            );
                        }
                        r.move_tab_to_area(child, to, None, window, cx);
                        assert_eq!(
                            r.tabs.iter().find(|t| t.id == child).unwrap().area,
                            to,
                            "a resolved orphan must not keep blocking unrelated navigation"
                        );
                    }
                    return;
                }
                if route == "menu" {
                    r.work_at.set(Some(gpui::Bounds {
                        origin: Default::default(),
                        size: gpui::size(gpui::px(1800.), gpui::px(1000.)),
                    }));
                    r.run_deed(
                        crate::view::menu::Deed::Tab(crate::view::menu::tabs::TabDeed::MoveToward(
                            guild_session::layout::Direction::Right,
                        )),
                        &crate::view::menu::Target::Tab {
                            id: moved,
                            path: None,
                            session: true,
                            index: 0,
                            of: 1,
                            neighbours: [true; 4],
                        },
                        window,
                        cx,
                    );
                } else if route == "drop" || route == "drop-after" {
                    r.reorder_tab(moved, target, route != "drop-after", window, cx)
                } else {
                    r.move_tab_to_area(moved, to, None, window, cx)
                }
                let related: Vec<_> = r
                    .tabs
                    .iter()
                    .filter(|t| {
                        t.bodies().any(|b| {
                            b.session() == Some(origin)
                                || matches!(b,Body::Process{run:r} if *r==run)
                        })
                    })
                    .collect();
                assert_eq!(
                    related.len(),
                    3,
                    "{route}: same three related bodies, no duplicate/new run"
                );
                assert!(
                    related.iter().all(|t| t.area == to),
                    "{route}: moving either member must carry origin and both exact-run views"
                );
                for id in [child, other] {
                    assert_eq!(
                        r.tabs.iter().find(|t| t.id == id).unwrap().area,
                        from,
                        "{route}: unrelated tab moved"
                    );
                }
                if let Some((tab, pane, original)) = kept {
                    let t = r.tabs.iter().find(|t| t.id == tab).unwrap();
                    assert_eq!(t.area, from);
                    assert!(matches!(
                        t.panes.body_of(pane),
                        Some(Body::Settings {
                            section: crate::view::settings::Section::General
                        })
                    ));
                    assert_eq!(t.panes.focused(), pane);
                    assert!(
                        related.iter().any(|t| t.panes.body_of(original).is_some()),
                        "native body keeps its pane identity"
                    );
                } else {
                    assert!(related.iter().any(|t| t.id == parent));
                    assert!(related.iter().any(|t| t.id == process));
                }
                assert!(related.iter().any(|t| t.id == duplicate));
                assert_eq!(
                    r.processes.runs[&run].value.as_ref().unwrap()["review_ledger"]["unchanged"],
                    true
                );
            })
        });
        if route.starts_with("closed-")
            || matches!(
                route,
                "unloaded"
                    | "legacy-unloaded"
                    | "legacy-partial"
                    | "orphan-unrelated"
                    | "foreign-target"
                    | "foreign-member"
                    | "foreign-only-members"
                    | "drop-self"
            )
        {
            continue;
        }
        guild_session::store::flush();
        let persisted = guild_session::project_state::load(dir.path());
        let saved = persisted
            .seats
            .values()
            .find_map(|s| s.areas.as_ref())
            .expect("divided seat persisted");
        let count = saved.strips[&to]
            .tabs
            .iter()
            .flat_map(|t| {
                t.panes_or_one()
                    .bodies()
                    .into_iter()
                    .map(|(_, b)| b.clone())
                    .collect::<Vec<_>>()
            })
            .filter(|b| {
                matches!(b,guild_session::Shows::Process{run:r} if *r==run)
                    || matches!(b,guild_session::Shows::Session(s) if *s==origin)
            })
            .count();
        assert_eq!(
            count, 3,
            "{route}: persisted placement must retain all group bodies"
        );
    }
}
