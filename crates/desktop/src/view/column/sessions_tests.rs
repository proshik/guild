//! The column's Sessions mode drawn in a whole window: sections by urgency,
//! Settled folded and open, the arrows, Enter and the speed search.
use super::*;
use gpui::{AppContext, TestAppContext};
use guild_session::status::{Kind, ReportOutcome, Reported};
use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord, Settings};

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn top(cx: &mut gpui::VisualTestContext, selector: String) -> Option<gpui::Pixels> {
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
        .map(|bounds| bounds.top())
}

#[gpui::test]
fn the_sessions_page_lists_by_urgency_settles_and_takes_keys(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::the_sessions_page_lists_by_urgency_settles_and_takes_keys"
    )) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        let seat = Seat { workspace, project };
        let make = |desk: &mut Desk, name: &str, runs: Runs, ago: u64| {
            let id = desk.add_session(SessionRecord::new(
                name.into(),
                seat,
                dir.path().into(),
                runs,
            ));
            desk.rename_session(id, name);
            let changed = ReportOutcome {
                changed: true,
                began: false,
                ended: false,
            };
            desk.observe_reported_activity(id, changed, now_ms() - ago);
            id
        };
        let claude = Runs::Claude(SessionMode::Terminal);
        let codex = Runs::Codex(SessionMode::Terminal);
        let waiting = make(&mut desk, "Fix the watcher", codex, 9 * 60_000);
        let old_work = make(&mut desk, "Review the parser", claude, 6 * 60_000);
        let new_work = make(&mut desk, "Add a search index", claude, 60_000);
        let done = make(&mut desk, "Session five", claude, 12 * 60_000);
        let stale = make(&mut desk, "Try the old walker", codex, 4 * DAY_MS);
        let ids = [waiting, old_work, new_work, done, stale];
        let root = cx.new(|cx| {
            let mut r = Root::from_state(
                Settings {
                    panels: Some(Default::default()),
                    column_mode: guild_session::settings::ColumnMode::Sessions,
                    ..Default::default()
                },
                desk,
                cx,
            );
            r.isolated_render = true;
            r.active_workspace = workspace;
            r.active_project = Some(project);
            r.restored_workspaces.insert(workspace);
            for id in ids {
                r.adopt(
                    seat,
                    project,
                    crate::Body::Broken {
                        session: id,
                        reason: "Fixture without a provider".into(),
                    },
                    cx,
                );
            }
            r.board
                .report(waiting, Some(Reported::Blocked(Some(Kind::Question))));
            r.board.report(old_work, Some(Reported::Working));
            r.board.report(new_work, Some(Reported::Working));
            r.board.report(done, Some(Reported::Done));
            r.settled.pasts.push(crate::root::settled::SettledPast {
                project,
                past: guild_session::history::Past {
                    harness: guild_session::HarnessKind::Claude,
                    id: "closed-1".into(),
                    title: "Migrate the CI cache".into(),
                    touched: std::time::SystemTime::now() - std::time::Duration::from_secs(86_400),
                },
            });
            r
        });
        made = Some((root.clone(), ids));
        gpui_component::Root::new(root, window, cx)
    });
    let (root, [waiting, old_work, new_work, done, stale]) = made.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    cx.run_until_parked();

    // Sections by urgency; an idle tab four days old is Settled, not Idle.
    let section = |cx: &mut gpui::VisualTestContext, name: &str| {
        top(cx, format!("column-sessions-section-{name}"))
    };
    let (w, k, d) = (
        section(cx, "Waiting").expect("Waiting for you"),
        section(cx, "Working").expect("Working"),
        section(cx, "Done").expect("Done"),
    );
    assert!(
        section(cx, "Idle").is_none(),
        "the only idle tab has settled"
    );
    assert!(w < k && k < d);
    let row = |cx: &mut gpui::VisualTestContext, id: guild_session::SessionId| {
        top(cx, format!("column-session-{id}"))
    };
    let rows = [waiting, new_work, old_work, done].map(|id| row(cx, id).expect("listed"));
    assert!(
        rows.windows(2).all(|pair| pair[0] < pair[1]),
        "waiting, then the newer working, the older, then done: {rows:?}"
    );
    assert!(row(cx, stale).is_none(), "Settled is folded");
    assert!(cx.debug_bounds("column-settled-row-closed-1").is_none());

    // Settled opens on a click, with the old tab and the closed session.
    let head = cx.debug_bounds("column-settled-head").expect("Settled");
    cx.simulate_click(head.center(), Default::default());
    cx.run_until_parked();
    assert!(row(cx, stale).is_some());
    assert!(cx.debug_bounds("column-settled-row-closed-1").is_some());
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx)
                .session_row_name(&crate::session_list::Key::Closed {
                    harness: guild_session::HarnessKind::Claude,
                    id: "closed-1".into()
                }),
            "Migrate the CI cache",
            "a closed session without a name is labelled by its history title"
        );
    });

    // Settled is apart at the foot: under the list, never above half the page.
    let bounds = |cx: &mut gpui::VisualTestContext, name: &str| {
        cx.debug_bounds(Box::leak(name.to_owned().into_boxed_str()))
    };
    let list = bounds(cx, "column-sessions").expect("the list");
    let foot = bounds(cx, "column-settled").expect("Settled's zone");
    let page = bounds(cx, "column-sessions-page").expect("the page");
    assert!(list.bottom() <= foot.top(), "Settled is under the list");
    assert!(
        (foot.bottom() - page.bottom()).abs() < px(1.),
        "pinned to the foot: {foot:?} in {page:?}"
    );

    // A long Settled stops at half the page and its rows scroll inside;
    // the list keeps the rest.
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            for n in 0..10 {
                r.settled.pasts.push(crate::root::settled::SettledPast {
                    project: r.active_project.unwrap(),
                    past: guild_session::history::Past {
                        harness: guild_session::HarnessKind::Claude,
                        id: format!("long-{n}"),
                        title: format!("Old work {n}"),
                        touched: std::time::SystemTime::now()
                            - std::time::Duration::from_secs(3_600 * (n + 2)),
                    },
                });
            }
            cx.notify();
        })
    });
    cx.run_until_parked();
    let page = bounds(cx, "column-sessions-page").expect("the page");
    let foot = bounds(cx, "column-settled").expect("Settled's zone");
    let rows = bounds(cx, "column-settled-rows").expect("Settled's rows");
    assert!(
        foot.size.height <= page.size.height / 2. + px(1.),
        "capped at half: {foot:?} of {page:?}"
    );
    assert!(
        (foot.bottom() - page.bottom()).abs() < px(1.) && rows.bottom() <= foot.bottom(),
        "at the foot, the rows scrolling inside it: {rows:?} in {foot:?} of {page:?}"
    );
    assert!(
        bounds(cx, "column-sessions").is_some_and(|list| list.bottom() <= foot.top()),
        "the list keeps the rest"
    );
    // The wheel over Settled scrolls its rows and nothing else.
    let head = bounds(cx, "column-settled-head").expect("Settled's heading");
    let last = bounds(cx, "column-settled-row-long-9").expect("the oldest row");
    assert!(
        last.top() >= foot.bottom(),
        "the oldest row starts out of view"
    );
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: rows.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-2000.))),
        touch_phase: gpui::TouchPhase::Moved,
        ..Default::default()
    });
    cx.run_until_parked();
    let scrolled = bounds(cx, "column-settled-row-long-9").expect("the oldest row");
    assert!(
        scrolled.bottom() <= foot.bottom() + px(1.) && scrolled.top() >= head.bottom(),
        "scrolled into Settled's view: {scrolled:?} in {foot:?}"
    );
    assert_eq!(
        bounds(cx, "column-settled-head"),
        Some(head),
        "the heading stays"
    );
    assert_eq!(bounds(cx, "column-settled"), Some(foot), "the foot stays");
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.settled.pasts.truncate(1);
            cx.notify();
        })
    });
    cx.run_until_parked();

    // A chevron collapses Working into the stack above Settled: its rows
    // leave the list, the arrows skip them, and the choice is kept.
    let working = bounds(cx, "column-sessions-section-Working").expect("Working");
    cx.simulate_click(working.center(), Default::default());
    cx.run_until_parked();
    assert!(row(cx, new_work).is_none() && row(cx, old_work).is_none());
    let list = bounds(cx, "column-sessions").expect("the list");
    let stacked = bounds(cx, "column-sessions-section-Working").expect("in the stack");
    let foot = bounds(cx, "column-settled").expect("Settled's zone");
    assert!(
        list.bottom() <= stacked.top() && stacked.bottom() <= foot.top(),
        "the collapsed heading is pinned between the list and Settled"
    );
    assert!(bounds(cx, "column-sessions-stack").is_some_and(|stack| stack.bottom() <= foot.top()));
    let saved = || {
        guild_session::store::flush();
        guild_session::Store::<Settings>::new(crate::paths::settings_file())
            .load()
            .expect("the settings file reads")
            .session_folds
    };
    let folds = saved();
    assert!(
        folds
            .collapsed
            .contains(&guild_session::settings::SessionSection::Working),
        "the chevron's choice is on disk"
    );
    assert!(folds.settled_open, "and so is Settled's");
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert!(
            r.settings
                .session_folds
                .collapsed
                .contains(&guild_session::settings::SessionSection::Working)
        );
        assert!(
            r.settings.session_folds.settled_open,
            "Settled's state is kept too"
        );
        let rows = r.session_rows();
        assert!(!rows.contains(&crate::session_list::Key::Open(new_work)));
        assert!(rows.contains(&crate::session_list::Key::Open(stale)));
    });

    // A session in front that a collapsed section hides is not the chosen
    // row: Enter must not open what is not drawn.
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.go_to_session(new_work, cx);
            r.session_cursor = None;
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert_eq!(r.front_session(), Some(new_work));
        assert_eq!(r.session_chosen(), None, "nothing drawn is chosen");
    });

    // Its heading in the stack expands it back into its place.
    let stacked = bounds(cx, "column-sessions-section-Working").expect("in the stack");
    cx.simulate_click(stacked.center(), Default::default());
    cx.run_until_parked();
    assert!(row(cx, new_work).is_some());
    assert!(bounds(cx, "column-sessions-stack").is_none());
    assert!(
        section(cx, "Working").unwrap() < section(cx, "Done").unwrap(),
        "back in its place, above Done"
    );
    assert!(saved().collapsed.is_empty(), "expanded on disk too");

    // The arrows choose, Enter opens.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            r.session_cursor = None;
            r.active_tab = None;
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("down down");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).session_cursor,
            Some(crate::session_list::Key::Open(new_work))
        );
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(root.read(cx).front_session(), Some(new_work)));

    // The speed search chooses by name, as a list's search does.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("p a r s e r");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).session_cursor,
            Some(crate::session_list::Key::Open(old_work))
        );
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(root.read(cx).front_session(), Some(old_work));
        assert!(root.read(cx).session_speed.is_none());
    });

    // A row's menu, from the right button.
    let at = cx
        .debug_bounds(Box::leak(
            format!("column-session-{waiting}").into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_mouse_down(at.center(), gpui::MouseButton::Right, Default::default());
    cx.run_until_parked();
    let target = cx.update(|_, cx| {
        root.read(cx)
            .context_menu
            .as_ref()
            .map(|m| m.target.clone())
    });
    assert_eq!(
        target,
        Some(crate::view::menu::Target::SessionRow {
            key: crate::session_list::Key::Open(waiting)
        })
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    // The filter: an agent unticked leaves its rows out; Show All brings
    // them back.
    let filter = cx.update(|_, cx| {
        let r = root.read(cx);
        crate::view::menu::Target::SessionFilter {
            workspace: r.active_workspace,
            projects: r.session_filter_projects(r.active_workspace),
        }
    });
    let pick = |cx: &mut gpui::VisualTestContext, pick| {
        let filter = filter.clone();
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.run_deed(
                    crate::view::menu::Deed::SessionFilter(pick),
                    &filter,
                    window,
                    cx,
                )
            })
        });
        cx.run_until_parked();
    };
    pick(
        cx,
        crate::view::menu::sessions::FilterPick::Agent(crate::agents::Agent::Codex),
    );
    assert!(
        row(cx, waiting).is_none(),
        "the Codex session is filtered out"
    );
    assert!(row(cx, done).is_some());
    let head = cx
        .debug_bounds("session-filter")
        .expect("the heading's filter");
    cx.simulate_click(head.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx)
                .context_menu
                .as_ref()
                .map(|m| m.target.clone()),
            Some(filter.clone())
        )
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    pick(cx, crate::view::menu::sessions::FilterPick::All);
    assert!(row(cx, waiting).is_some());

    // Show in Projects: the mode switches and the session's tab is in front.
    let row_target = crate::view::menu::Target::SessionRow {
        key: crate::session_list::Key::Open(done),
    };
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.run_deed(
                crate::view::menu::Deed::Session(
                    crate::view::menu::sessions::SessionDeed::ShowInProjects,
                ),
                &row_target,
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!root.read(cx).sessions_mode());
        assert_eq!(root.read(cx).front_session(), Some(done));
    });
    assert!(row(cx, waiting).is_none());
    assert!(section(cx, "Waiting").is_none());
}

/// A Settled conversation of another project continues in that project:
/// it comes to the front first, so the strip, the tools and the saved
/// selection are its own. Two repositories of one name are filtered apart.
#[gpui::test]
fn continuing_a_settled_conversation_selects_its_project_and_names_filter_apart(
    cx: &mut TestAppContext,
) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::continuing_a_settled_conversation_selects_its_project_and_names_filter_apart"
    )) {
        return;
    }
    let root_dir = tempfile::tempdir().unwrap();
    let a_dir = root_dir.path().join("one/app");
    let b_dir = root_dir.path().join("two/app");
    std::fs::create_dir_all(&a_dir).unwrap();
    std::fs::create_dir_all(&b_dir).unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let a = desk.open_in(workspace, &a_dir);
        let b = desk.open_in(workspace, &b_dir);
        let codex = desk.add_session(SessionRecord::new(
            "In A".into(),
            Seat {
                workspace,
                project: a,
            },
            a_dir.clone(),
            Runs::Codex(SessionMode::Terminal),
        ));
        let mut settings = Settings {
            panels: Some(Default::default()),
            column_mode: guild_session::settings::ColumnMode::Sessions,
            ..Default::default()
        };
        // Nothing real is started if the harness is there: the prefix
        // runs, and exits.
        settings.launch_prefixes.claude = "/usr/bin/true".into();
        let root = cx.new(|cx| {
            let mut r = Root::from_state(settings, desk, cx);
            r.isolated_render = true;
            r.active_workspace = workspace;
            r.active_project = Some(a);
            r.restored_workspaces.insert(workspace);
            r.adopt(
                Seat {
                    workspace,
                    project: a,
                },
                a,
                crate::Body::Broken {
                    session: codex,
                    reason: "Fixture without a provider".into(),
                },
                cx,
            );
            r.board.report(codex, Some(Reported::Working));
            r.settled.pasts.push(crate::root::settled::SettledPast {
                project: b,
                past: guild_session::history::Past {
                    harness: guild_session::HarnessKind::Claude,
                    id: "6f1c2d3e-4b5a-4c7d-8e9f-0a1b2c3d4e5f".into(),
                    title: "Migrate the CI cache".into(),
                    touched: std::time::SystemTime::now() - std::time::Duration::from_secs(3_600),
                },
            });
            r.settings.session_folds.settled_open = true;
            r
        });
        made = Some((root.clone(), a, b, codex));
        gpui_component::Root::new(root, window, cx)
    });
    let (root, a, b, codex) = made.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    cx.run_until_parked();

    // Two repositories called `app`: each its own check, told apart by
    // its folder, and unticking one leaves the other's rows.
    let projects = cx.update(|_, cx| {
        let r = root.read(cx);
        r.session_filter_projects(r.active_workspace)
    });
    assert_eq!(projects.len(), 2);
    assert_ne!(projects[0].0, projects[1].0, "keys are the repositories'");
    assert_ne!(projects[0].1, projects[1].1, "labels tell them apart");
    let a_key = cx.update(|_, cx| {
        let r = root.read(cx);
        Root::repository_key(a, &r.column_groups_for(r.active_workspace), &r.desk)
    });
    let index = projects.iter().position(|(key, _)| *key == a_key).unwrap();
    let filter = cx.update(|_, cx| {
        let r = root.read(cx);
        crate::view::menu::Target::SessionFilter {
            workspace: r.active_workspace,
            projects: r.session_filter_projects(r.active_workspace),
        }
    });
    let pick = |cx: &mut gpui::VisualTestContext, pick| {
        let filter = filter.clone();
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.run_deed(
                    crate::view::menu::Deed::SessionFilter(pick),
                    &filter,
                    window,
                    cx,
                )
            })
        });
        cx.run_until_parked();
    };
    pick(cx, crate::view::menu::sessions::FilterPick::Project(index));
    assert!(
        top(cx, format!("column-session-{codex}")).is_none(),
        "A's session is filtered out"
    );
    assert!(
        cx.debug_bounds("column-settled-row-6f1c2d3e-4b5a-4c7d-8e9f-0a1b2c3d4e5f")
            .is_some(),
        "B's conversation stays"
    );
    pick(cx, crate::view::menu::sessions::FilterPick::All);

    // Continue B's conversation with A in front: B comes to the front.
    let row = cx
        .debug_bounds("column-settled-row-6f1c2d3e-4b5a-4c7d-8e9f-0a1b2c3d4e5f")
        .unwrap();
    cx.simulate_click(row.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert_eq!(r.active_project, Some(b), "the conversation's own project");
        // With Claude on this machine's PATH a tab opens (its prefix runs
        // and exits); it must be B's, and that very conversation.
        if let Some(front) = r.front_session() {
            let record = r.desk.session(front).unwrap();
            assert_eq!(record.project, b, "a tab it opened is B's");
            assert_eq!(
                record.id.to_string(),
                "6f1c2d3e-4b5a-4c7d-8e9f-0a1b2c3d4e5f",
                "and it resumes the chosen conversation"
            );
        }
    });
}

/// Continuing a conversation whose previous process is still going after
/// its tab closed waits for it, off the window's thread, and starts exactly
/// one; one that never goes is refused with a word. A Settled row and the
/// start screen's Recent row take this one path.
#[gpui::test]
fn continuing_waits_for_the_previous_process_and_never_starts_a_second(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::continuing_waits_for_the_previous_process_and_never_starts_a_second"
    )) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        let mut settings = Settings {
            panels: Some(Default::default()),
            column_mode: guild_session::settings::ColumnMode::Sessions,
            ..Default::default()
        };
        settings.launch_prefixes.claude = "/usr/bin/true".into();
        // Two conversations whose tabs were just closed: their records are
        // closed, their processes still going.
        let mut closed = |title: &str| {
            let mut record = SessionRecord::new(
                title.into(),
                Seat { workspace, project },
                dir.path().into(),
                Runs::Claude(SessionMode::Terminal),
            );
            record.open = false;
            desk.add_session(record)
        };
        let (settled, recent) = (closed("Settled one"), closed("Recent one"));
        let root = cx.new(|cx| {
            let mut r = Root::from_state(settings, desk, cx);
            r.isolated_render = true;
            r.active_workspace = workspace;
            r.active_project = Some(project);
            r.restored_workspaces.insert(workspace);
            r.settled.pasts.push(crate::root::settled::SettledPast {
                project,
                past: guild_session::history::Past {
                    harness: guild_session::HarnessKind::Claude,
                    id: settled.to_string(),
                    title: "Settled one".into(),
                    touched: std::time::SystemTime::now(),
                },
            });
            r.settings.session_folds.settled_open = true;
            r
        });
        made = Some((root.clone(), settled, recent));
        gpui_component::Root::new(root, window, cx)
    });
    let (root, settled, recent) = made.unwrap();
    cx.executor().allow_parking();
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    cx.run_until_parked();
    let holding = |cx: &mut gpui::VisualTestContext, id: guild_session::SessionId| {
        cx.update(|_, cx| {
            root.read(cx)
                .tabs
                .iter()
                .filter(|tab| tab.sessions().any(|held| held == id))
                .count()
        })
    };
    let warned = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            root.read(cx)
                .notices
                .visible()
                .any(|(_, notice)| notice.text.contains("still closing"))
        })
    };

    // The Settled row's process goes a moment after the click: the click
    // waits for it, and two clicks start one process once it has gone.
    let tag = settled.to_string();
    crate::terminal::retiring::begin(&tag);
    let row = cx
        .debug_bounds(Box::leak(
            format!("column-settled-row-{settled}").into_boxed_str(),
        ))
        .expect("the closed conversation is Settled");
    let asked = std::time::Instant::now();
    let reaper = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        crate::terminal::retiring::end(&tag);
    });
    cx.simulate_click(row.center(), Default::default());
    cx.simulate_click(row.center(), Default::default());
    cx.run_until_parked();
    reaper.join().unwrap();
    assert!(
        asked.elapsed() >= std::time::Duration::from_millis(300),
        "the click waited for the last process"
    );
    assert_eq!(holding(cx, settled), 1, "once it went, exactly one tab");
    assert!(!warned(cx));

    // The Recent row's process never goes: refused, and told why.
    let tag = recent.to_string();
    crate::terminal::retiring::begin(&tag);
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.start_deed(
                crate::start::Deed::Resume {
                    past: guild_session::history::Past {
                        harness: guild_session::HarnessKind::Claude,
                        id: recent.to_string(),
                        title: "Recent one".into(),
                        touched: std::time::SystemTime::now(),
                    },
                },
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    crate::terminal::retiring::end(&tag);
    assert_eq!(holding(cx, recent), 0, "no second process from Recent");
    assert!(warned(cx), "the person is told why");

    // Asked in one workspace and answered after a switch to another: the
    // press is dropped rather than opened in a seat nobody is looking at.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let other = r.desk.add_workspace("Other", None).unwrap();
            r.start_deed(
                crate::start::Deed::Resume {
                    past: guild_session::history::Past {
                        harness: guild_session::HarnessKind::Claude,
                        id: recent.to_string(),
                        title: "Recent one".into(),
                        touched: std::time::SystemTime::now(),
                    },
                },
                window,
                cx,
            );
            r.active_workspace = other;
        })
    });
    cx.run_until_parked();
    assert_eq!(holding(cx, recent), 0, "nothing opens after a switch");
}
