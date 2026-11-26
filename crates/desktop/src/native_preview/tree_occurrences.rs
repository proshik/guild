//! Isolated production trees/editor; the fixture's Git answer is read before startup.
use gpui::{App, AppContext, Entity, Window};
use std::path::Path;

pub struct Snapshot {
    status: crate::git::status::StatusMap,
    commits: Vec<crate::git::log::Commit>,
    files: Vec<crate::git::log::FileChange>,
    code: String,
    proposed: String,
    blame: Vec<crate::git::blame::Run>,
}
impl Snapshot {
    /// Called before the native event loop, keeping Git IO off its thread.
    pub fn read(fixture: &Path) -> anyhow::Result<Self> {
        let status = crate::git::status::StatusMap::from_entries(crate::git::status::parse(
            &crate::git::status(fixture)?,
        )?);
        let commits = crate::git::log::parse(&crate::git::log(
            fixture,
            &Default::default(),
            &Default::default(),
            0,
            10,
        )?)?;
        anyhow::ensure!(!commits.is_empty(), "fixture needs a commit");
        let files =
            crate::git::log::files_of(&crate::git::commit_files(fixture, &commits[0].sha)?)?;
        Ok(Self {
            code: std::fs::read_to_string(fixture.join("example.rs"))?,
            proposed: std::fs::read_to_string(fixture.join("proposed.rs")).unwrap_or_default(),
            blame: crate::git::blame::parse(&crate::git::blame(fixture, Path::new("example.rs"))?)?,
            status,
            commits,
            files,
        })
    }
}

pub fn root(
    fixture: &Path,
    snapshot: Snapshot,
    window: &mut Window,
    cx: &mut App,
) -> anyhow::Result<Entity<crate::Root>> {
    let root = super::tooltip_root(fixture, window, cx)?;
    let scene = std::env::var("GUILD_TREE_SCENE").unwrap_or_default();
    // The marker trial has a layout-independent test alias for the existing
    // production SaveFile/Undo/Redo actions; no product shortcut is added.
    if scene == "git-markers" {
        cx.bind_keys([
            gpui::KeyBinding::new("f6", gpui_component::input::Undo, Some("guild")),
            gpui::KeyBinding::new("f7", gpui_component::input::Redo, Some("guild")),
            gpui::KeyBinding::new("f8", crate::actions::SaveFile, Some("guild")),
        ]);
    }
    let code = snapshot.code.clone();
    let blame = snapshot.blame.clone();
    root.update(cx, |root, cx| {
        root.settings.terminal_font_size = 15.;
        root.settings.autosave = false;
        let project = root.active_project.expect("fixture project");
        root.git_status.insert(project, Ok(snapshot.status));
        root.ensure_log(cx);
        let sha = snapshot.commits[0].sha.clone();
        let log = root.logs.get_mut(&project).unwrap();
        log.commits = snapshot.commits.clone();
        log.opened = Some(sha.clone());
        log.files.insert(sha.clone(), Ok(snapshot.files.clone()));
        log.file_tree = crate::files_tree::Tree::new(&snapshot.files);
        log.graph = std::rc::Rc::new(crate::log_graph::Graph::of(&snapshot.commits));
        let push = root.push.of_mut(project);
        push.ahead = Some(Ok(crate::git::Ahead {
            branch: Some("main".into()),
            remotes: Vec::new(),
            upstream: None,
            commits: snapshot.commits,
        }));
        push.opened = Some(sha.clone());
        push.files.insert(sha, Ok(snapshot.files));
        root.refresh_changes(project, cx);
        // A proposal owns immutable input; do not also open a working editor
        // whose eventual load would attach the normal live-buffer follower.
        if scene != "proposal" {
            root.open_file(Path::new("example.rs"), window, cx);
        }
        root.reveal_tool(guild_session::Tool::Explorer, cx);
        root.with_panels(|panels| {
            panels.left.size = 320.;
            panels.bottom.size = 460.;
        });
        if std::env::var("GUILD_TREE_SCENE").as_deref() == Ok("log") {
            root.reveal_tool(guild_session::Tool::Git, cx);
        }
        match scene.as_str() {
            "commit" => root.reveal_tool(guild_session::Tool::Commit, cx),
            "diff" | "unified" => {
                root.open_diff(
                    Some("example.rs".into()),
                    guild_session::Base::Head,
                    window,
                    cx,
                );
                if scene == "unified"
                    && let Some(crate::Body::Diff { tab, .. }) = root.active_body()
                {
                    tab.update(cx, |tab, cx| {
                        if tab.mode() != guild_session::DiffView::Unified {
                            tab.toggle_mode(cx);
                        }
                    });
                }
                root.hide_dock(guild_session::Dock::Left, cx);
            }
            "proposal" => {
                let look = root.file_look();
                let ui = root.ui();
                let view = cx.new(|cx| {
                    crate::diff_tab::DiffTab::open_proposal(
                        fixture,
                        "example.rs".into(),
                        Some(code.as_bytes().to_vec()),
                        snapshot.proposed.as_bytes().to_vec(),
                        guild_session::DiffView::SideBySide,
                        look,
                        ui,
                        cx,
                    )
                });
                root.hear_diff(&view, window, cx);
                let id = root.adopt(
                    root.seat_for_project(project),
                    project,
                    crate::Body::Diff {
                        path: Some("example.rs".into()),
                        base: guild_session::Base::Proposal,
                        tab: view,
                    },
                    cx,
                );
                root.active_tab = Some(id);
                root.hide_dock(guild_session::Dock::Left, cx);
            }
            "restored-annotate" => {
                let id = root.restore_annotate_tab(project, "example.rs".into(), cx);
                root.active_tab = Some(id);
                if let Some(crate::Body::Annotate { tab, .. }) = root.active_body() {
                    tab.update(cx, |tab, cx| tab.set_runs(Ok(blame.clone()), cx));
                }
                root.hide_dock(guild_session::Dock::Left, cx);
            }
            "readonly" => {
                let origin = root.native_occurrence_library(project, fixture.join("library"));
                root.open_file_from(origin, Path::new("library.rs"), window, cx);
            }
            "markdown" => {
                root.open_file(Path::new("notes.md"), window, cx);
                if let Some(crate::Body::File {
                    presentation: Some(p),
                    ..
                }) = root.active_body()
                {
                    p.restore(guild_session::markdown::Mode::Split, cx);
                }
                root.hide_dock(guild_session::Dock::Left, cx);
            }
            "search" => {
                root.ensure_search(window, cx);
                root.searches.get_mut(&project).unwrap().query.text = "total".into();
                root.restart_search(project, cx);
                root.reveal_tool(guild_session::Tool::Search, cx);
                root.hide_dock(guild_session::Dock::Left, cx);
            }
            "find" => {
                root.hide_dock(guild_session::Dock::Left, cx);
                root.native_occurrence_usages(&code, window, cx);
            }
            "find-popup" => root.open_find_in_files(window, cx),
            "everywhere" => {
                let weak = cx.entity().downgrade();
                window.on_next_frame(move |window, cx| {
                    let _ = weak.update(cx, |root, cx| {
                        root.everywhere_pressed(crate::everywhere::Among::Text, window, cx)
                    });
                });
            }
            _ => {}
        }
        cx.notify();
    });
    if scene == "diff-window" {
        let sha = root.read(cx).logs.values().next().unwrap().commits[0]
            .sha
            .clone();
        let weak = root.downgrade();
        window.on_next_frame(move |_, cx| {
            let _ = weak.update(cx, |root, cx| {
                root.open_diff_window(
                    root.active_project.unwrap(),
                    sha,
                    vec!["example.rs".into()],
                    0,
                    cx,
                )
            });
        });
    }
    if std::env::var("GUILD_TREE_SCENE").as_deref() == Ok("push") {
        let weak = root.downgrade();
        window.on_next_frame(move |_, cx| {
            let _ = weak.update(cx, |root, cx| {
                let project = root.active_project.unwrap();
                let state = root.push.of(project).unwrap();
                let ahead = state.ahead.clone();
                let opened = state.opened.clone();
                root.open_push(cx);
                // Normal opening clears the previous worker answer. Restore
                // only this fixture's snapshot; no worker/destination exists.
                let state = root.push.of_mut(project);
                state.ahead = ahead;
                state.opened = opened;
            });
        });
    }
    // Diagnostic Root skips the integration clock. Exercise only local source
    // search/preview publication, with no providers, hooks or Git poll added.
    let weak = root.downgrade();
    window
        .spawn(cx, async move |cx| {
            loop {
                if !matches!(
                    cx.update(|_, cx| weak
                        .update(cx, |root, cx| {
                            root.poll_diffs(cx);
                            root.poll_search(cx);
                            root.poll_picker(cx);
                            if scene == "find" {
                                root.ask_find_preview(cx);
                            }
                            if scene == "inline"
                                && let Some(crate::Body::File { tab, .. }) = root.active_body()
                                && tab.read(cx).editor().is_some()
                                && !tab.read(cx).blame_enabled()
                            {
                                tab.update(cx, |tab, cx| {
                                    tab.native_occurrence_blame(blame.clone(), cx)
                                });
                            }
                        })
                        .is_ok()),
                    Ok(true)
                ) {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
            }
        })
        .detach();
    Ok(root)
}
