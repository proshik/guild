use super::*;
use gpui::{AppContext, TestAppContext};
#[test]
fn isolated_blame_log_navigation() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["root::annotate::tests::", "--ignored", "--test-threads=1"])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_BLAME_LOG_TEST", "1")
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
#[ignore = "runs in isolated_blame_log_navigation"]
fn annotation_explicitly_reveals_the_log_face(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_BLAME_LOG_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            root
        });
        root.update(cx, |root, cx| {
            root.show_git_face(guild_session::GitFace::Console, cx);
            root.open_commit_in_log(&"a".repeat(40), cx);
            assert_eq!(root.git_face(), guild_session::GitFace::Log);
            let commit = |sha: String| crate::git::log::Commit {
                sha,
                parents: vec![],
                author: "A".into(),
                email: "a@invalid".into(),
                time: 1,
                refs: vec![],
                summary: "S".into(),
                body: String::new(),
            };
            let sha = "b".repeat(40);
            let state = root.logs.get_mut(&project).unwrap();
            state.filter = crate::git::Filter {
                text: "excluded".into(),
                author: Some("other".into()),
                since: Some("today".into()),
            };
            state.commits = (0..201).map(|i| commit(format!("{i:040x}"))).collect();
            let old_epoch = state.epoch;
            root.reveal_commit(project, &sha, Some("old/а.rs".into()), Some(23), cx);
            let intent = root.logs[&project].navigation.clone().unwrap();
            root.scoped_log_landed(
                project,
                crate::root::log::PageReply {
                    epoch: old_epoch,
                    skip: 0,
                    commits: Ok(vec![commit("c".repeat(40))]),
                },
                cx,
            );
            assert_eq!(
                root.logs[&project].navigation,
                Some(intent.clone()),
                "late filtered page cannot override reveal"
            );
            let files = vec![
                crate::git::log::FileChange {
                    path: "new/а.rs".into(),
                    from: Some("old/а.rs".into()),
                    mark: crate::theme::Mark::Modified,
                },
                crate::git::log::FileChange {
                    path: "unrelated.rs".into(),
                    from: None,
                    mark: crate::theme::Mark::Added,
                },
            ];
            root.reveal_commit_landed(
                project,
                crate::root::log::RevealReply {
                    intent: intent.clone(),
                    commit: Ok(commit(sha.clone())),
                    files: Ok(files.clone()),
                },
                cx,
            );
            let state = &root.logs[&project];
            assert_eq!(state.commits.len(), 1);
            assert_eq!(state.opened.as_deref(), Some(sha.as_str()));
            assert_eq!(state.file_selection.selected, [PathBuf::from("new/а.rs")]);
            assert!(state.filter.author.is_none() && state.filter.since.is_none());
            root.reveal_commit(
                project,
                &"d".repeat(40),
                Some("missing.rs".into()),
                None,
                cx,
            );
            let missing = root.logs[&project].navigation.clone().unwrap();
            root.reveal_commit_landed(
                project,
                crate::root::log::RevealReply {
                    intent,
                    commit: Ok(commit(sha)),
                    files: Ok(files.clone()),
                },
                cx,
            );
            assert_eq!(
                root.logs[&project].navigation,
                Some(missing.clone()),
                "superseded commit reply is ignored"
            );
            root.reveal_commit_landed(
                project,
                crate::root::log::RevealReply {
                    intent: missing,
                    commit: Ok(commit("d".repeat(40))),
                    files: Ok(files),
                },
                cx,
            );
            assert!(root.logs[&project].file_selection.selected.is_empty());
            assert!(root.logs[&project].navigation_error.is_some());
            root.reveal_commit(project, &"e".repeat(40), Some("new/а.rs".into()), None, cx);
            let unavailable = root.logs[&project].navigation.clone().unwrap();
            root.active_project = None;
            root.reveal_commit_landed(
                project,
                crate::root::log::RevealReply {
                    intent: unavailable,
                    commit: Err(GitError::Failed("missing commit".into())),
                    files: Ok(Vec::new()),
                },
                cx,
            );
            assert!(
                root.active_project.is_none(),
                "late reply must not switch the active project"
            );
            assert!(root.logs[&project].opened.is_none());
            assert!(root.logs[&project].error.is_some());
            root.active_project = Some(project);
            let epoch = root.logs[&project].epoch;
            root.reveal_commit(project, &"0".repeat(40), Some("new/а.rs".into()), None, cx);
            assert_eq!(
                root.logs[&project].epoch, epoch,
                "zero hash never navigates"
            );
            root.stop_every_process(cx);
        });
    });
}
