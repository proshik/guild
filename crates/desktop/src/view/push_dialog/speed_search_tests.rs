use super::*;
use gpui::{Modifiers, TestAppContext};

#[test]
fn isolated_push_speed_search_regression() {
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::push_dialog::speed_search_tests::push_search_owns_enter",
            "--ignored",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", dir.path())
        .env("GUILD_SPEED_SEARCH_TEST", "1")
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
#[ignore = "run through isolated_push_speed_search_regression"]
fn push_search_owns_enter(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_SPEED_SEARCH_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    let mut root = None;
    let mut dialog = None;
    let mut id = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        id = Some(project);
        let entity = cx.new(|cx| {
            let mut r = Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            r.active_project = Some(project);
            let s = r.push.of_mut(project);
            let mut commits = [("1111111", "One change"), ("2222222", "Second change")]
                .map(|(sha, summary)| crate::git::log::Commit {
                    sha: sha.into(),
                    parents: vec![],
                    author: "Fixture".into(),
                    email: "fixture@example.invalid".into(),
                    time: 0,
                    refs: vec![],
                    summary: summary.into(),
                    body: String::new(),
                })
                .to_vec();
            let last = commits.pop().unwrap();
            for i in 0..60 {
                let mut c = commits[0].clone();
                c.sha = format!("filler-{i}");
                c.summary = format!("Filler {i}");
                commits.push(c);
            }
            commits.push(last);
            s.ahead = Some(Ok(crate::git::Ahead {
                branch: Some("topic".into()),
                remotes: vec!["origin".into()],
                upstream: Some("origin/main".into()),
                commits,
            }));
            s.opened = Some("1111111".into());
            for sha in ["1111111", "2222222"] {
                s.files.insert(
                    sha.into(),
                    Ok(["a.rs", "b.rs", "c.rs"]
                        .map(|p| FileChange {
                            path: p.into(),
                            mark: theme::Mark::Modified,
                            from: None,
                        })
                        .to_vec()),
                );
            }
            r
        });
        let view =
            cx.new(|cx| PushDialog::new(entity.downgrade(), project, entity.read(cx).theme, cx));
        *entity.read(cx).push_dialog.borrow_mut() = Some(view.downgrade());
        root = Some(entity);
        dialog = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.simulate_resize(gpui::size(px(1000.), px(620.)));
    let root = root.unwrap();
    let dialog = dialog.unwrap();
    let project = id.unwrap();
    let click = |cx: &mut gpui::VisualTestContext, name| {
        let b = cx.debug_bounds(name).expect(name);
        cx.simulate_click(b.center(), Modifiers::default());
    };
    click(cx, "push-commit-0");
    cx.simulate_keystrokes("s e c");
    assert!(
        cx.debug_bounds("speed-search").is_some(),
        "commit list has independent type-to-search focus"
    );
    cx.update(|_, cx| {
        assert_eq!(
            dialog.read(cx).speed_query(PushList::Commits),
            "sec",
            "both Push surfaces must retain the active field's entire query"
        )
    });
    cx.simulate_keystrokes("cmd-a");
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("missing target".into()))
    });
    cx.simulate_keystrokes("cmd-v");
    cx.update(|_, cx| {
        assert_eq!(
            dialog.read(cx).speed_query(PushList::Commits),
            "missing target"
        )
    });
    cx.simulate_keystrokes("cmd-z");
    cx.update(|_, cx| assert_eq!(dialog.read(cx).speed_query(PushList::Commits), "sec"));
    cx.update(|_, cx| {
        let r = root.read(cx);
        let s = r.push.of(project).unwrap();
        assert_eq!(s.selected.shas, ["2222222"]);
        assert_eq!(s.opened.as_deref(), Some("2222222"));
        assert_eq!(s.destination().unwrap().branch, "topic");
        assert!(!s.running);
        assert!(
            dialog.read(cx).commit_scroll.offset().y < px(0.),
            "offscreen matching commit must be revealed"
        );
    });
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            let s = r.push.of_mut(project);
            s.ahead
                .as_mut()
                .unwrap()
                .as_mut()
                .unwrap()
                .commits
                .last_mut()
                .unwrap()
                .sha = "3333333".into();
            s.files.insert(
                "3333333".into(),
                Ok(["a.rs", "b.rs", "c.rs"]
                    .map(|p| FileChange {
                        path: p.into(),
                        mark: theme::Mark::Modified,
                        from: None,
                    })
                    .to_vec()),
            );
        });
        dialog.update(cx, |_, cx| cx.notify());
    });
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).push.of(project).unwrap().opened.as_deref(),
            Some("3333333"),
            "refresh reconciles inspection to the current matching commit"
        )
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(!root.read(cx).push.of(project).unwrap().running));
    cx.simulate_keystrokes("z z");
    cx.update(|_, cx| assert_eq!(dialog.read(cx).speed.as_ref().unwrap().search.query, "zz"));
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(!root.read(cx).push.of(project).unwrap().running));
    click(cx, "push-file-0");
    cx.simulate_keystrokes("cmd-a c . r s");
    assert!(cx.debug_bounds("speed-search").is_some());
    cx.update(|_, cx| {
        assert_eq!(dialog.read(cx).speed_query(PushList::Files), "c.rs");
        assert_eq!(
            dialog.read(cx).selected_file.as_deref(),
            Some(std::path::Path::new("c.rs")),
            "the match is the selection and the diff's target while the query is up"
        );
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert!(!root.read(cx).push.of(project).unwrap().running);
        // The match is the selection — decision 54.
        assert_eq!(
            dialog.read(cx).file_selection.selected,
            [std::path::PathBuf::from("c.rs")]
        );
        assert_eq!(
            dialog.read(cx).file_selection.focus.as_deref(),
            Some(std::path::Path::new("c.rs")),
            "Enter leaves the match focused"
        );
    });
    // A click while the query is up is the selection; a redraw must not
    // put the match back. The row's click handler runs before the next
    // frame, as it does when press and release come together.
    cx.simulate_keystrokes("c");
    cx.update(|_, cx| {
        dialog.update(cx, |d, cx| {
            let order: Vec<_> = ["a.rs", "b.rs", "c.rs"]
                .map(std::path::PathBuf::from)
                .to_vec();
            d.file_selection
                .click(std::path::Path::new("a.rs"), &order, false, false);
            d.selected_file = d.file_selection.action_target().cloned();
            cx.notify();
        })
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.update(|_, cx| assert!(dialog.read(cx).speed.is_some(), "the query is still up"));
    cx.update(|_, cx| {
        assert_eq!(
            dialog.read(cx).file_selection.selected,
            [std::path::PathBuf::from("a.rs")]
        )
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("z z");
    cx.update(|_, cx| assert_eq!(dialog.read(cx).speed.as_ref().unwrap().search.query, "zz"));
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(!root.read(cx).push.of(project).unwrap().running));
    // A target field owns its ordinary text; Escape must not trigger Push either.
    cx.update(|window, cx| {
        dialog.update(cx, |d, cx| {
            d.open_popup(Popup::Target, gpui::point(px(50.), px(50.)), cx);
            window.refresh();
        })
    });
    cx.simulate_keystrokes("cmd-a t o p i c");
    cx.update(|_, cx| {
        assert_eq!(dialog.read(cx).target_text, "topic");
        assert!(dialog.read(cx).speed.is_none());
    });
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(dialog.read(cx).popup.is_none()));
    cx.update(|_, cx| assert!(!root.read(cx).push.of(project).unwrap().running));
}

#[test]
fn isolated_preview_crosses_folded_folders_regression() {
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::push_dialog::speed_search_tests::preview_crosses_folded_folders",
            "--ignored",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", dir.path())
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
#[ignore = "run through isolated_preview_crosses_folded_folders_regression"]
fn preview_crosses_folded_folders(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(dir.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    for (message, text) in [("base", "old fixture\n"), ("change", "new fixture\n")] {
        for path in ["a/A.rs", "b/B.rs", "c/C.rs"] {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            message,
        ]);
    }
    let sha = git(&["rev-parse", "HEAD"]);
    let mut root = None;
    let mut dialog = None;
    let mut id = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        id = Some(project);
        let entity = cx.new(|cx| {
            let mut r = Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            r.active_project = Some(project);
            let s = r.push.of_mut(project);
            let commits = vec![crate::git::log::Commit {
                sha: sha.clone(),
                parents: vec![],
                author: "Fixture".into(),
                email: "fixture@example.invalid".into(),
                time: 0,
                refs: vec![],
                summary: "Fixture change".into(),
                body: String::new(),
            }];
            s.ahead = Some(Ok(crate::git::Ahead {
                branch: Some("topic".into()),
                remotes: vec!["origin".into()],
                upstream: Some("origin/main".into()),
                commits,
            }));
            s.opened = Some(sha.clone());
            s.files.insert(
                sha.clone(),
                Ok(["a/A.rs", "b/B.rs", "c/C.rs"]
                    .map(|p| FileChange {
                        path: p.into(),
                        mark: theme::Mark::Modified,
                        from: None,
                    })
                    .to_vec()),
            );
            r
        });
        let view =
            cx.new(|cx| PushDialog::new(entity.downgrade(), project, entity.read(cx).theme, cx));
        *entity.read(cx).push_dialog.borrow_mut() = Some(view.downgrade());
        root = Some(entity);
        dialog = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.simulate_resize(gpui::size(px(1000.), px(620.)));
    let root = root.unwrap();
    let dialog = dialog.unwrap();
    let project = id.unwrap();
    let click = |cx: &mut gpui::VisualTestContext, name| {
        let b = cx.debug_bounds(name).expect(name);
        cx.simulate_click(b.center(), Modifiers::default());
    };

    // Actual tree click and preview toggle create the production subscription.
    click(cx, "push-file-1");
    click(cx, "push-preview");
    for _ in 0..200 {
        cx.run_until_parked();
        let ready = cx.update(|_, cx| {
            let preview = dialog.read(cx).preview.clone().unwrap();
            preview.update(cx, |tab, cx| {
                tab.poll(cx);
                tab.can_step_change(true)
            })
        });
        if ready {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let original_preview = cx.update(|_, cx| dialog.read(cx).preview.clone().unwrap());
    original_preview.update(cx, |tab, cx| {
        if tab.mode() != guild_session::DiffView::Unified {
            tab.toggle_mode(cx);
        }
        tab.viewer.ratio = 0.62;
        tab.toggle_authors(1, cx);
    });
    // Establish a real prior change owner through the preview control before
    // stepping across the folded directory boundary.
    click(cx, "diff-next");
    cx.update(|_, cx| {
        assert_eq!(
            dialog
                .read(cx)
                .preview
                .as_ref()
                .unwrap()
                .read(cx)
                .current_change(),
            Some(0)
        )
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let d = dialog.read(cx);
        assert_eq!(
            d.selected_file.as_deref(),
            Some(std::path::Path::new("a/A.rs")),
            "change button must retain preview file"
        );
        assert!(d.preview_on, "change button must retain preview visibility");
    });
    dialog.update(cx, |d, cx| {
        d.folded.insert("b".into());
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "diff-file-next");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let d = dialog.read(cx);
        assert_eq!(
            d.selected_file.as_deref(),
            Some(std::path::Path::new("b/B.rs"))
        );
        assert_eq!(
            d.preview_key.as_ref().map(|k| k.1.as_path()),
            Some(std::path::Path::new("b/B.rs"))
        );
        assert_eq!(
            d.preview.as_ref().unwrap().read(cx).stepping(),
            Some((1, 3))
        );
        assert!(!d.folded.contains("b"));
        let preview = d.preview.as_ref().unwrap();
        assert_eq!(preview.entity_id(), original_preview.entity_id());
        assert_eq!(preview.read(cx).mode(), guild_session::DiffView::Unified);
        assert_eq!(preview.read(cx).viewer.ratio, 0.62);
        assert_eq!(preview.read(cx).authors.enabled, [false, true]);
        assert!(
            !preview
                .read(cx)
                .documents
                .contains_key(&PathBuf::from("a/A.rs"))
        );
        assert_eq!(preview.read(cx).files()[0].path, PathBuf::from("b/B.rs"));
    });
    dialog.update(cx, |d, cx| {
        d.folded.insert("a".into());
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "diff-file-previous");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let d = dialog.read(cx);
        assert_eq!(
            d.selected_file.as_deref(),
            Some(std::path::Path::new("a/A.rs"))
        );
        assert_eq!(
            d.preview.as_ref().unwrap().read(cx).stepping(),
            Some((0, 3))
        );
        assert!(!d.folded.contains("a"));
        // Root remains alive only for isolated read-only state, never Push.
        assert!(root.read(cx).desk.project(project).is_some());
    });
    click(cx, "diff-file-counter");
    cx.run_until_parked();
    click(cx, "diff-file-choice-2");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let d = dialog.read(cx);
        assert_eq!(
            d.selected_file.as_deref(),
            Some(std::path::Path::new("c/C.rs"))
        );
        assert_eq!(
            d.preview_key.as_ref(),
            Some(&(sha.clone(), PathBuf::from("c/C.rs")))
        );
        let preview = d.preview.as_ref().unwrap();
        assert_eq!(preview.entity_id(), original_preview.entity_id());
        assert_eq!(preview.read(cx).stepping(), Some((2, 3)));
        assert_eq!(preview.read(cx).mode(), guild_session::DiffView::Unified);
        assert_eq!(preview.read(cx).viewer.ratio, 0.62);
        assert!(!root.read(cx).push.of(project).unwrap().running);
    });
}
