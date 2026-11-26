//! A worktree's merge request in its row and in the status bar — GLD-063.
//! The model is landed by hand: nothing here starts glab.
use super::*;
use gpui::{AppContext, Entity, TestAppContext};
use guild_session::Desk;
use guild_session::merge_requests::status::{MergeRequest, State};

const URL: &str = "https://gitlab.example.com/team/app/-/merge_requests/299";

struct Host {
    root: Entity<Root>,
    row: Row,
    workspace: guild_session::WorkspaceId,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(crate::scale::rem_size(13.0));
        self.root.update(cx, |root, cx| {
            div()
                .w(px(232.))
                .flex()
                .flex_col()
                .child(root.render_row(self.workspace, &self.row, cx))
                .children(root.render_merge_request_status())
        })
    }
}

fn mr() -> MergeRequest {
    MergeRequest {
        iid: 299,
        title: "[OC-760] fix(slack): replies".into(),
        web_url: URL.into(),
        state: State::Open,
        target_branch: "master".into(),
        draft: false,
        conflict: true,
        mergeable: false,
        needs_rebase: false,
        checking: false,
        block: None,
        approvals: Some(guild_session::merge_requests::status::Approvals {
            given: 1,
            required: 1,
            satisfied: true,
        }),
        unresolved_threads: Some(6),
        pipeline: None,
        updated_at: None,
        finished_at: None,
        author: None,
    }
}

#[gpui::test]
fn a_worktree_s_merge_request_is_drawn_opened_and_explained(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::a_worktree_s_merge_request_is_drawn_opened_and_explained"
    )) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().to_path_buf();
    let common = folder.join(".git");
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, &folder);
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.isolated_render = true;
            root.active_project = Some(project);
            root.settings.gitlab_merge_requests = true;
            root.worktrees.insert(
                common.clone(),
                vec![crate::git::worktrees::Worktree {
                    path: folder.clone(),
                    branch: Some("feat".into()),
                    head: Some("abc".into()),
                    ahead: Some(2),
                    push: Some(crate::git::worktrees::Pushed {
                        remote: "origin".into(),
                        url: Some("git@gitlab.example.com:team/app.git".into()),
                        branch: "feat".into(),
                    }),
                    ..Default::default()
                }],
            );
            let now = std::time::Instant::now();
            let model = root.forge_model_mut();
            model.set_enabled(true, now);
            model.inputs(
                &common,
                vec![crate::forge::Candidate {
                    worktree: folder.clone(),
                    url: "git@gitlab.example.com:team/app.git".into(),
                    branch: "feat".into(),
                }],
                now,
            );
            let ask = model.take_due(now).remove(0);
            model.land(
                crate::forge::Answer {
                    repo: ask.repo,
                    candidates: ask.candidates,
                    generation: ask.generation,
                    accounts: Vec::new(),
                    resolutions: vec![(
                        crate::forge::Candidate {
                            worktree: folder.clone(),
                            url: "git@gitlab.example.com:team/app.git".into(),
                            branch: "feat".into(),
                        },
                        crate::forge::Resolution::Asked {
                            host: "gitlab.example.com".into(),
                            project: "team/app".into(),
                        },
                    )],
                    batches: vec![crate::forge::Batch {
                        host: "gitlab.example.com".into(),
                        project: "team/app".into(),
                        result: Ok(vec![("feat".into(), Ok(Some(mr())))]),
                    }],
                },
                now,
            );
            root
        });
        let row = Row::Opened {
            project,
            title: crate::column::Title {
                name: "oc-760-slack".into(),
                primary: false,
            },
            branch: Some("feat".into()),
            committed: Some(1),
            lines: None,
        };
        let view = cx.new(|_| Host {
            root,
            row,
            workspace,
        });
        made = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(232.), px(200.)));
    let host = made.unwrap();
    cx.run_until_parked();

    // The icon in its verdict's colour with the threads badge, the second
    // line's `↑2`, and the status bar's link.
    assert!(cx.debug_bounds("worktree-mr-glyph").is_some(), "MR glyph");
    assert!(
        cx.debug_bounds("worktree-mr-badge-Threads").is_some(),
        "threads badge"
    );
    assert!(cx.debug_bounds("worktree-ahead").is_some(), "unpushed");
    assert!(cx.debug_bounds("footer-merge-request").is_some());

    // The hover is the path to merge.
    let glyph = cx.debug_bounds("worktree-mr-glyph").unwrap();
    cx.simulate_mouse_move(glyph.center(), None, gpui::Modifiers::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("worktree-mr-card").is_some(),
        "the hover card"
    );

    // A refresh after that read fails: the card says when the read was and
    // why the refresh failed, and the long line stays inside the card. A
    // tooltip is laid out at its minimum size; a line given no width
    // wrapped past the card's edge (docs/traps.md, a line wraps only
    // against a width its element has).
    cx.simulate_mouse_move(
        gpui::point(px(1.), px(199.)),
        None,
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    host.update(cx, |host, cx| {
        host.root.update(cx, |root, cx| {
            let later = std::time::Instant::now() + std::time::Duration::from_secs(200);
            let model = root.forge_model_mut();
            model.trigger(None, crate::forge::Trigger::Tick, later);
            let ask = model.take_due(later).remove(0);
            let mut failed = crate::forge::Answer {
                repo: ask.repo,
                candidates: ask.candidates.clone(),
                generation: ask.generation,
                accounts: Vec::new(),
                resolutions: vec![(
                    ask.candidates[0].clone(),
                    crate::forge::Resolution::Asked {
                        host: "gitlab.example.com".into(),
                        project: "team/app".into(),
                    },
                )],
                batches: Vec::new(),
            };
            failed.batches.push(crate::forge::Batch {
                host: "gitlab.example.com".into(),
                project: "team/app".into(),
                result: Ok(vec![(
                    "feat".into(),
                    Err(guild_session::merge_requests::glab::Error::Failed(
                        "glab: 500 Internal Server Error (HTTP 500) while reading the \
                         merge request's discussions, page two of three"
                            .into(),
                    )),
                )]),
            });
            assert!(model.land(failed, later));
            cx.notify();
        });
    });
    cx.run_until_parked();
    cx.simulate_mouse_move(glyph.center(), None, gpui::Modifiers::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    let card = cx.debug_bounds("worktree-mr-card").expect("the hover card");
    let failure = cx
        .debug_bounds("worktree-mr-card-failure")
        .expect("the failure line");
    // A hundred and twenty characters do not fit one line of the card: a
    // line laid out as one is the bug, its rest painted below the edge.
    assert!(
        failure.size.height > px(30.),
        "the failure line wraps inside its box: {failure:?}"
    );
    assert!(
        failure.bottom() <= card.bottom() && failure.right() <= card.right(),
        "{failure:?} outside {card:?}"
    );

    // A click opens the MR. Each click must replace a sentinel, so one
    // that does nothing cannot pass on the other's URL (code review r1,
    // docs+tests-9).
    const SENTINEL: &str = "about:gld-063-sentinel";
    cx.update(|_, cx| cx.open_url(SENTINEL));
    assert_eq!(cx.opened_url().as_deref(), Some(SENTINEL));
    cx.simulate_click(glyph.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    assert_eq!(cx.opened_url().as_deref(), Some(URL));

    // So does the status bar's `!299 ↗`.
    cx.update(|_, cx| cx.open_url(SENTINEL));
    assert_eq!(cx.opened_url().as_deref(), Some(SENTINEL));
    let link = cx.debug_bounds("footer-merge-request-link").unwrap();
    cx.simulate_click(link.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    assert_eq!(cx.opened_url().as_deref(), Some(URL));

    // Off: the row and the status bar are as without it, `↑2` stays.
    host.update(cx, |host, cx| {
        host.root.update(cx, |root, cx| {
            root.settings.gitlab_merge_requests = false;
            root.poll_forge(cx);
        });
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("worktree-mr-glyph").is_none());
    assert!(cx.debug_bounds("footer-merge-request").is_none());
    assert!(
        cx.debug_bounds("worktree-ahead").is_some(),
        "git's, not GitLab's"
    );
}

/// GLD-063: the repository's header draws its merge
/// requests' state from the model — queued, checking, couldn't refresh,
/// checked — and nothing when the setting is off; a problem of the
/// repository is on its header, never on a row.
#[gpui::test]
fn the_repository_header_draws_its_merge_requests_state(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::the_repository_header_draws_its_merge_requests_state"
    )) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("app");
    let feat = dir.path().join("feat");
    std::fs::create_dir_all(&main).unwrap();
    std::fs::create_dir_all(&feat).unwrap();
    let common = main.join(".git");
    let url = "git@gitlab.example.com:team/app.git";
    let candidate = crate::forge::Candidate {
        worktree: feat.clone(),
        url: url.into(),
        branch: "feat".into(),
    };
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, &main);
        desk.set_repository(project, Some(common.clone()));
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.isolated_render = true;
            root.settings.gitlab_merge_requests = true;
            let tree = |path: &std::path::Path, branch: &str, main_branch: bool| {
                crate::git::worktrees::Worktree {
                    path: path.to_path_buf(),
                    branch: Some(branch.into()),
                    head: Some("abc".into()),
                    main_branch,
                    push: Some(crate::git::worktrees::Pushed {
                        remote: "origin".into(),
                        url: Some(url.into()),
                        branch: branch.into(),
                    }),
                    ..Default::default()
                }
            };
            root.worktrees.insert(
                common.clone(),
                vec![tree(&main, "main", true), tree(&feat, "feat", false)],
            );
            let now = std::time::Instant::now();
            let model = root.forge_model_mut();
            model.set_enabled(true, now);
            model.inputs(&common, vec![candidate.clone()], now);
            root
        });
        entity = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    cx.simulate_resize(gpui::size(px(900.), px(500.)));
    let root = entity.unwrap();
    cx.run_until_parked();
    let only = |cx: &mut gpui::VisualTestContext, shown: &str| {
        for (state, selector) in [
            ("Queued", "project-header-mr-Queued"),
            ("Checking", "project-header-mr-Checking"),
            ("Checked", "project-header-mr-Checked"),
            ("Failed", "project-header-mr-Failed"),
        ] {
            let found = cx.debug_bounds(selector).is_some();
            assert_eq!(found, state == shown, "{state} while {shown} is due");
        }
    };
    only(cx, "Queued");

    let ask = root.update(cx, |root, cx| {
        let ask = root
            .forge_model_mut()
            .take_due(std::time::Instant::now())
            .remove(0);
        cx.notify();
        ask
    });
    cx.run_until_parked();
    only(cx, "Checking");

    // glab is not logged in to the host: the header says so, the row does not.
    root.update(cx, |root, cx| {
        root.forge_model_mut().land(
            crate::forge::Answer {
                repo: ask.repo.clone(),
                candidates: ask.candidates.clone(),
                generation: ask.generation,
                accounts: Vec::new(),
                resolutions: vec![(
                    candidate.clone(),
                    crate::forge::Resolution::Why(crate::forge::Why::NotLoggedIn(
                        "gitlab.example.com".into(),
                    )),
                )],
                batches: Vec::new(),
            },
            std::time::Instant::now(),
        );
        cx.notify();
    });
    cx.run_until_parked();
    only(cx, "Failed");
    assert!(cx.debug_bounds("worktree-mr-glyph").is_none());
    let failed = cx.debug_bounds("project-header-mr-Failed").unwrap();
    cx.simulate_mouse_move(failed.center(), None, gpui::Modifiers::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("guild-control-tooltip").is_some(),
        "the reason in the hover"
    );
    cx.simulate_mouse_move(
        gpui::point(px(600.), px(450.)),
        None,
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();

    // Asked again, and answered: checked.
    root.update(cx, |root, cx| {
        let later = std::time::Instant::now() + std::time::Duration::from_secs(400);
        let model = root.forge_model_mut();
        model.trigger(None, crate::forge::Trigger::Tick, later);
        let ask = model.take_due(later).remove(0);
        model.land(
            crate::forge::Answer {
                repo: ask.repo.clone(),
                candidates: ask.candidates.clone(),
                generation: ask.generation,
                accounts: Vec::new(),
                resolutions: vec![(
                    candidate.clone(),
                    crate::forge::Resolution::Asked {
                        host: "gitlab.example.com".into(),
                        project: "team/app".into(),
                    },
                )],
                batches: vec![crate::forge::Batch {
                    host: "gitlab.example.com".into(),
                    project: "team/app".into(),
                    result: Ok(vec![("feat".into(), Ok(Some(mr())))]),
                }],
            },
            later,
        );
        cx.notify();
    });
    cx.run_until_parked();
    only(cx, "Checked");
    // Code review r1, bugs+impl-2: one move straight onto the quiet ✓ —
    // revealed by that very move — and a still pointer show its hover
    // (docs/traps.md, a cursor tooltip can miss the only move that reveals
    // its control).
    let checked = cx.debug_bounds("project-header-mr-Checked").unwrap();
    cx.simulate_mouse_move(checked.center(), None, gpui::Modifiers::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("guild-control-tooltip").is_some(),
        "when it was checked, on the first move"
    );
    cx.simulate_mouse_move(
        gpui::point(px(600.), px(450.)),
        None,
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();

    // Off: nothing.
    root.update(cx, |root, cx| {
        root.settings.gitlab_merge_requests = false;
        root.poll_forge(cx);
    });
    cx.run_until_parked();
    only(cx, "none");
}

/// GLD-063: a merge request whose author is glab's
/// account wears a person after its number, with its hover; another's
/// wears none.
#[gpui::test]
fn the_person_s_own_merge_request_is_marked(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::the_person_s_own_merge_request_is_marked"
    )) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().to_path_buf();
    let common = folder.join(".git");
    let url = "git@gitlab.example.com:team/app.git";
    let candidate = crate::forge::Candidate {
        worktree: folder.clone(),
        url: url.into(),
        branch: "feat".into(),
    };
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, &folder);
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.isolated_render = true;
            root.settings.gitlab_merge_requests = true;
            let now = std::time::Instant::now();
            let model = root.forge_model_mut();
            model.set_enabled(true, now);
            model.inputs(&common, vec![candidate.clone()], now);
            root
        });
        let row = Row::Opened {
            project,
            title: crate::column::Title {
                name: "feat".into(),
                primary: false,
            },
            branch: Some("feat".into()),
            committed: Some(1),
            lines: None,
        };
        let view = cx.new(|_| Host {
            root,
            row,
            workspace,
        });
        made = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(232.), px(200.)));
    let host = made.unwrap();
    let land = |cx: &mut gpui::VisualTestContext, author: u64, account: u64, at: u64| {
        host.update(cx, |host, cx| {
            host.root.update(cx, |root, cx| {
                let now = std::time::Instant::now() + std::time::Duration::from_secs(at);
                let model = root.forge_model_mut();
                model.trigger(None, crate::forge::Trigger::Tick, now);
                let ask = model.take_due(now).remove(0);
                model.land(
                    crate::forge::Answer {
                        repo: ask.repo,
                        candidates: ask.candidates,
                        generation: ask.generation,
                        resolutions: vec![(
                            candidate.clone(),
                            crate::forge::Resolution::Asked {
                                host: "gitlab.example.com".into(),
                                project: "team/app".into(),
                            },
                        )],
                        batches: vec![crate::forge::Batch {
                            host: "gitlab.example.com".into(),
                            project: "team/app".into(),
                            result: Ok(vec![(
                                "feat".into(),
                                Ok(Some(MergeRequest {
                                    author: Some(author),
                                    ..mr()
                                })),
                            )]),
                        }],
                        accounts: vec![("gitlab.example.com".into(), account)],
                    },
                    now,
                );
                cx.notify();
            });
        });
        cx.run_until_parked();
    };
    land(cx, 42, 42, 0);
    let mark = cx
        .debug_bounds("worktree-mr-author")
        .expect("the person's own MR is marked");
    let number = cx.debug_bounds("activity-age").unwrap();
    assert!(number.right() <= mark.left(), "after the number");
    cx.simulate_mouse_move(mark.center(), None, gpui::Modifiers::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("guild-control-tooltip").is_some(),
        "its hover"
    );
    cx.simulate_mouse_move(
        gpui::point(px(1.), px(199.)),
        None,
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();

    land(cx, 7, 42, 200);
    assert!(
        cx.debug_bounds("worktree-mr-author").is_none(),
        "another's MR is not marked"
    );
    assert!(cx.debug_bounds("worktree-mr-glyph").is_some());
}
