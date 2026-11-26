use super::*;
use gpui::TestAppContext;

#[test]
fn isolated_log_tree_state() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::log::tests::commit_and_project_scope",
            "--ignored",
            "--test-threads=1",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_LOG_TREE_TEST", "1")
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
#[ignore = "runs in isolated_log_tree_state"]
fn commit_and_project_scope(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_LOG_TREE_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let second = desk.open_in(desk.active_workspace(), other.path());
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            root
        });
        root.update(cx, |root, cx| {
            root.ensure_log(cx);
            let commit = |sha: &str| Commit {
                sha: sha.into(),
                parents: vec![],
                author: String::new(),
                email: String::new(),
                time: 0,
                refs: vec![],
                summary: String::new(),
                body: String::new(),
            };
            root.log_landed(
                project,
                0,
                Ok(vec![commit("one"), commit("middle"), commit("two")]),
                cx,
            );
            let files = ["src/a.rs", "src/deep/b.rs", "README.md"]
                .map(|p| FileChange {
                    path: p.into(),
                    from: None,
                    mark: crate::theme::Mark::Modified,
                })
                .to_vec();
            root.select_log_commit(project, "one", false, false);
            root.select_log_commit(project, "two", true, false);
            assert_eq!(root.logs[&project].opened.as_deref(), Some("one,two"));
            root.log_files_landed(project, "two".into(), Ok(files.clone()), cx);
            assert!(
                root.logs[&project].file_tree.paths().is_empty(),
                "late single reply must not paint combined selection"
            );
            root.log_files_landed(project, "one,two".into(), Ok(files.clone()), cx);
            assert_eq!(root.logs[&project].file_tree.paths().len(), 3);
            root.select_log_commit(project, "one", false, false);
            root.select_log_commit(project, "two", false, true);
            assert_eq!(
                root.logs[&project].opened.as_deref(),
                Some("one,middle,two")
            );
            assert_eq!(root.logs[&project].cursor.as_deref(), Some("two"));
            root.select_log_commit(project, "two", false, false);
            root.select_log_commit(project, "one", true, false);
            root.log_landed(project, 0, Ok(vec![commit("one"), commit("middle")]), cx);
            assert_eq!(
                root.logs[&project].opened.as_deref(),
                Some("one"),
                "refresh keeps only surviving commits"
            );
            root.log_landed(project, 0, Ok(vec![commit("one"), commit("two")]), cx);
            root.select_log_commit(project, "two", true, false);
            root.select_log_commit(project, "one", true, false);
            assert_eq!(root.logs[&project].opened.as_deref(), Some("two"));
            root.select_log_commit(project, "two", true, false);
            assert_eq!(root.logs[&project].opened, None);
            root.select_log_commit(project, "one", false, false);
            root.select_log_commit(project, "one", false, false);
            assert_eq!(
                root.logs[&project].opened.as_deref(),
                Some("one"),
                "repeated plain click stays selected"
            );
            root.log_landed(project, 0, Ok(vec![commit("two")]), cx);
            assert!(
                root.logs[&project].opened.is_none(),
                "filter removes absent selection"
            );
            root.log_landed(project, 0, Ok(vec![commit("one"), commit("two")]), cx);
            root.logs.get_mut(&project).unwrap().files.clear();
            root.open_log_commit(project, "one");
            root.log_files_landed(project, "one".into(), Ok(files.clone()), cx);
            // Push and duplicate requests share this reply path with Log.
            root.logs
                .get_mut(&project)
                .unwrap()
                .file_tree
                .toggle(Some("src/deep"));
            root.log_files_landed(project, "one".into(), Ok(files.clone()), cx);
            assert!(
                root.logs[&project].file_tree.folded.contains("src/deep"),
                "same-commit reply must preserve directory folding"
            );
            let state = root.logs.get_mut(&project).unwrap();
            state
                .file_selection
                .click(Path::new("README.md"), &state.file_paths(), false, false);
            root.log_files_landed(project, "one".into(), Ok(files.clone()), cx);
            assert_eq!(
                root.logs[&project].file_selection.selected,
                [std::path::PathBuf::from("README.md")]
            );
            root.log_files_landed(
                project,
                "one".into(),
                Err(GitError::Failed("temporary".into())),
                cx,
            );
            assert!(root.logs[&project].file_selection.selected.is_empty());
            assert!(
                root.file_speed_rows(crate::root::speed_search::FileHost::Log)
                    .is_empty()
            );
            root.log_files_landed(project, "one".into(), Ok(files.clone()), cx);
            assert!(root.logs[&project].file_tree.folded.contains("src/deep"));
            root.logs.get_mut(&project).unwrap().file_tree.toggle(None);
            root.log_files_landed(project, "one".into(), Ok(files.clone()), cx);
            assert!(
                root.logs[&project].file_tree.root_folded,
                "same-commit reply must preserve root folding"
            );
            root.log_files_landed(
                project,
                "one".into(),
                Err(GitError::Failed("temporary".into())),
                cx,
            );
            assert!(root.logs[&project].file_paths().is_empty());
            root.log_files_landed(project, "one".into(), Ok(files.clone()), cx);
            assert!(root.logs[&project].file_tree.root_folded);
            assert!(root.logs[&project].file_tree.folded.contains("src/deep"));
            root.logs.get_mut(&project).unwrap().file_tree.toggle(None);
            let state = root.logs.get_mut(&project).unwrap();
            state
                .file_selection
                .click(Path::new("src/a.rs"), &state.file_paths(), false, false);
            let replacement = vec![FileChange {
                path: "new/file.rs".into(),
                from: None,
                mark: crate::theme::Mark::Added,
            }];
            root.log_files_landed(project, "one".into(), Ok(replacement), cx);
            assert!(
                root.logs[&project].file_tree.folded.is_empty(),
                "obsolete compact directory keys must be removed"
            );
            assert!(
                root.logs[&project].file_selection.selected.is_empty(),
                "removed files must leave selection"
            );
            assert_eq!(
                root.logs[&project].file_paths(),
                [std::path::PathBuf::from("new/file.rs")]
            );
            root.log_files_landed(project, "one".into(), Ok(files.clone()), cx);
            let graph = root.logs[&project].graph.clone();
            let state = root.logs.get_mut(&project).unwrap();
            state
                .file_selection
                .click(Path::new("src/a.rs"), &state.file_paths(), false, false);
            root.fold_log_files(false, cx);
            assert!(root.logs[&project].file_paths().is_empty());
            assert!(root.logs[&project].file_selection.selected.is_empty());
            assert!(
                root.file_speed_rows(crate::root::speed_search::FileHost::Log)
                    .is_empty()
            );
            assert!(
                std::rc::Rc::ptr_eq(&graph, &root.logs[&project].graph),
                "folding must not rebuild ancestry"
            );
            root.active_project = Some(second);
            root.ensure_log(cx);
            assert!(!root.logs[&second].file_tree.root_folded);
            root.active_project = Some(project);
            assert!(
                root.logs[&project].file_tree.root_folded,
                "project owns its fold state"
            );
            root.open_log_commit(project, "two");
            assert!(!root.logs[&project].file_tree.root_folded);
            assert!(
                root.logs[&project].file_tree.rows.is_empty(),
                "no previous commit while loading"
            );
            root.log_files_landed(project, "one".into(), Ok(files.clone()), cx);
            assert!(
                root.logs[&project].file_tree.rows.is_empty(),
                "late files are cached but not displayed"
            );
            root.log_files_landed(project, "two".into(), Ok(files), cx);
            root.open_log_commit(project, "one");
            assert_eq!(
                root.logs[&project].file_paths().len(),
                3,
                "cached commit resets to expanded"
            );
            let state = root.logs.get_mut(&project).unwrap();
            state.file_tree.toggle(Some("src/deep"));
            let candidates = root.file_speed_rows(crate::root::speed_search::FileHost::Log);
            assert_eq!(
                candidates.len(),
                2,
                "speed search visits visible files only"
            );
            root.stop_every_process(cx);
        });
    });
}
