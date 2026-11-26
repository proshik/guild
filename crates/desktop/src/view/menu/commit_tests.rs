//! Real production menu/Commit rendering with fixture-only state and no Trash acceptance.
use super::*;
use crate::{
    Root, changes,
    git::status::{Entry as StatusEntry, Kind, StatusMap},
};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, px};
use std::path::Path;

#[test]
fn isolated_commit_menu_corrections() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::menu::commit_tests::",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_COMMIT_MENU_TEST", "1")
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
fn mount(cx: &mut TestAppContext) -> (tempfile::TempDir, Entity<Root>, &mut VisualTestContext) {
    mount_project(cx, false)
}
fn mount_project(
    cx: &mut TestAppContext,
    subproject: bool,
) -> (tempfile::TempDir, Entity<Root>, &mut VisualTestContext) {
    assert_eq!(std::env::var("GUILD_COMMIT_MENU_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success()
    );
    let project_path = if subproject {
        dir.path().join("project-name")
    } else {
        dir.path().to_path_buf()
    };
    std::fs::create_dir_all(&project_path).unwrap();
    for name in ["a.txt", "b.txt"] {
        std::fs::write(project_path.join(name), "fixture").unwrap();
    }
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), &project_path);
        let mut panels = guild_session::Panels::default();
        panels.left.open = true;
        panels.docks_mut().show(guild_session::Tool::Commit);
        let entity = cx.new(|cx| {
            let mut r = Root::from_state(
                guild_session::Settings {
                    panels: Some(panels),
                    ..Default::default()
                },
                desk,
                cx,
            );
            r.active_project = Some(project);
            r.isolated_render = true;
            r.git_status.insert(
                project,
                Ok(StatusMap::from_entries(
                    ["a.txt", "b.txt"]
                        .map(|name| StatusEntry {
                            path: name.into(),
                            kind: Kind::Modified,
                            staged: false,
                            unstaged: true,
                            is_dir: false,
                        })
                        .to_vec(),
                )),
            );
            r.refresh_changes(project, cx);
            r.ensure_message_box(window, cx);
            let state = r.changes_mut().unwrap();
            state
                .selection
                .key("a", &state.visible_paths(), true, false);
            window.focus(&state.focus, cx);
            r
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    frames(cx);
    (dir, root.unwrap(), cx)
}
fn frames(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|w, cx| w.simulate_next_frame(cx));
    }
    cx.run_until_parked();
}
fn aim(r: &mut Root, path: &Path, window: &mut Window, cx: &mut Context<Root>) {
    r.changes_context(path, window, cx);
    let state = r.changes().unwrap();
    let listed = state.list.iter().find(|l| l.row.path == path).unwrap();
    let target = Target::Change {
        path: path.into(),
        ticked: state.membership(path).0,
        unmerged: listed.row.unmerged,
        is_dir: listed.row.is_dir,
        unversioned: listed.section == changes::Section::Unversioned,
        gone: listed.row.mark == crate::theme::Mark::Deleted,
    };
    r.open_menu(
        target,
        Anchor::Pointer(gpui::point(px(360.), px(100.))),
        window,
        cx,
    );
}
fn highlight(r: &mut Root, deed: Deed, cx: &gpui::App) {
    let open = r.context_menu.as_ref().unwrap();
    let index = r
        .entries_for_open(open, cx)
        .iter()
        .position(|e| matches!(e,Entry::Item(_,d)|Entry::NamedItem(_,d) if *d==deed))
        .expect("requested live menu action");
    r.context_menu.as_mut().unwrap().lit = Some(index);
}

/// **The window hands its code face to the component as soon as it
/// resolves it** — a Markdown preview's code reads the component's
/// monospace family, and handing it over only when the face *changed*
/// left Menlo there: the window starts with the default face already in
/// `code_font`, so its first resolution changes nothing. Here, beside the
/// window this module already mounts with private state.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn the_component_draws_code_in_the_window_s_code_face(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    cx.update(|_, cx| {
        let face = root.read(cx).code_font.clone();
        assert!(!face.is_empty());
        assert_eq!(
            gpui_component::Theme::global(cx)
                .semantic_tokens()
                .typography
                .mono,
            face
        );
    });
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn discovery_cannot_retarget_highlighted_trash(cx: &mut TestAppContext) {
    let (dir, root, cx) = mount(cx);
    std::fs::create_dir(dir.path().join("new")).unwrap();
    std::fs::write(dir.path().join("new/file.txt"), "new").unwrap();
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let map = StatusMap::from_entries(vec![StatusEntry {
                path: "new".into(),
                kind: Kind::Untracked,
                staged: false,
                unstaged: true,
                is_dir: true,
            }]);
            let state = r.changes_mut().unwrap();
            state.list = changes::list(&map);
            state.directories.clear();
            state.selection = Default::default();
            r.git_status.insert(project, Ok(map));
            aim(r, Path::new("new"), w, cx);
            highlight(r, Deed::Change(ChangeDeed::Delete), cx);
            r.refresh_changes(project, cx); // actual asynchronous completion while the card stays open
        })
    });
    frames(cx);
    cx.update(|_, cx| {
        assert!(
            root.read(cx)
                .changes()
                .unwrap()
                .directories
                .get(Path::new("new"))
                .unwrap()
                .is_ok()
        )
    });
    cx.simulate_keystrokes("enter");
    frames(cx);
    assert!(
        cx.pending_prompt().is_some(),
        "Enter must still ask for Trash, never silently stage"
    );
    cx.simulate_prompt_answer("Cancel");
    frames(cx);
    assert!(dir.path().join("new/file.txt").exists());
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_changed_or_reduced_group_refuses_the_open_menu(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    let original = root.read_with(cx, |r, _| r.changes().unwrap().list.clone());
    for remove in [false, true] {
        cx.update(|w, cx| {
            root.update(cx, |r, cx| {
                let state = r.changes_mut().unwrap();
                state.list = original.clone();
                state
                    .selection
                    .key("a", &state.visible_paths(), true, false);
                aim(r, Path::new("a.txt"), w, cx);
                highlight(r, Deed::Change(ChangeDeed::CopyPaths), cx);
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()));
                let state = r.changes_mut().unwrap();
                if remove {
                    state.list.retain(|l| l.row.path != Path::new("b.txt"));
                    state.settle_selection();
                } else {
                    state
                        .selection
                        .click(Path::new("a.txt"), &state.visible_paths(), false, false);
                }
            })
        });
        cx.simulate_keystrokes("enter");
        frames(cx);
        assert!(
            cx.pending_prompt().is_none(),
            "no other command may replace Copy after group changes"
        );
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("sentinel")
            );
            assert!(
                root.read(cx)
                    .notices
                    .visible()
                    .any(|(_, n)| n.text.contains("Selection changed"))
            );
        });
    }
}

struct RestoreKeys(std::collections::BTreeMap<Named, String>);
impl Drop for RestoreKeys {
    fn drop(&mut self) {
        crate::actions::keymap::save(&self.0).unwrap();
    }
}
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn rendered_named_menu_shows_remapped_shortcut(cx: &mut TestAppContext) {
    let _restore = RestoreKeys(crate::actions::keymap::overrides());
    crate::actions::keymap::save(&std::collections::BTreeMap::from([(
        Named::DeleteFile,
        "ctrl-alt-t".into(),
    )]))
    .unwrap();
    let (_dir, root, cx) = mount(cx);
    cx.update(|w, cx| root.update(cx, |r, cx| aim(r, Path::new("a.txt"), w, cx)));
    frames(cx);
    assert!(
        cx.debug_bounds("popup-note-In the list, Space toggles commit inclusion.")
            .is_some(),
        "unbound Space remains discoverable in the rendered menu"
    );
    assert!(
        cx.debug_bounds("popup-shortcut-⌃⌥T").is_some(),
        "the actual rendered NamedItem must carry the configured hint"
    );
    assert!(
        cx.debug_bounds("popup-shortcut-⌘⌫").is_none(),
        "the replaced default must not be advertised"
    );
    assert!(
        cx.debug_bounds("popup-shortcut-⌘⌥D").is_some(),
        "the selected-group diff advertises its existing effective binding"
    );
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn empty_include_and_exclude_give_russian_feedback(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    for include in [true, false] {
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.settings.language = crate::i18n::Language::Russian;
                let state = r.changes_mut().unwrap();
                let rows = state.visible_paths();
                state
                    .selection
                    .click(Path::new("a.txt"), &rows, false, false);
                // A Command-click takes the row out and leaves the cursor on it.
                state
                    .selection
                    .click(Path::new("a.txt"), &rows, true, false);
                assert!(state.selection.selected.is_empty() && state.selection.focus.is_some());
                r.notices = Default::default();
                let before = r.notices.visible().count();
                r.changes_include_selected(Some(include), cx);
                assert!(
                    r.notices.visible().count() > before
                        || r.notices
                            .visible()
                            .any(|(_, n)| n.text == r.ui().text(Text::ChangesSelectRow)),
                    "an empty named action must explain how to select files"
                );
                assert!(
                    r.notices
                        .visible()
                        .any(|(_, n)| n.text == r.ui().text(Text::ChangesSelectRow))
                );
            })
        });
    }
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn hierarchy_parent_navigation_reveals_the_selected_row(cx: &mut TestAppContext) {
    let (dir, root, cx) = mount(cx);
    std::fs::create_dir(dir.path().join("new")).unwrap();
    for n in 0..150 {
        std::fs::write(dir.path().join(format!("new/{n:03}.txt")), "data").unwrap();
    }
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.git_status.insert(
                project,
                Ok(StatusMap::from_entries(vec![StatusEntry {
                    path: "new".into(),
                    kind: Kind::Untracked,
                    staged: false,
                    unstaged: true,
                    is_dir: true,
                }])),
            );
            r.refresh_changes(project, cx);
            r.changes_mut().unwrap().expanded.insert("new".into());
            w.focus(&r.changes().unwrap().focus, cx);
        })
    });
    frames(cx);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let row = r
                .file_speed_rows(crate::root::speed_search::FileHost::Commit)
                .into_iter()
                .find(|row| row.key == Path::new("new/149.txt"))
                .unwrap();
            let state = r.changes_mut().unwrap();
            state.selection.click(
                Path::new("new/149.txt"),
                &state.visible_paths(),
                false,
                false,
            );
            state.settle_selection();
            state
                .scroll
                .scroll_to_item(row.row, gpui::ScrollStrategy::Top);
            w.focus(&state.focus, cx);
            cx.notify();
        })
    });
    frames(cx);
    assert!(
        cx.debug_bounds("commit-row-new").is_none(),
        "the expanded parent starts well outside the viewport"
    );
    cx.simulate_keystrokes("left");
    frames(cx);
    let parent = cx
        .debug_bounds("commit-row-new")
        .expect("Left must bring the selected parent into view");
    let viewport = cx.debug_bounds("commit-list-viewport").unwrap();
    assert!(
        parent.top() >= viewport.top() && parent.bottom() <= viewport.bottom(),
        "selected parent {parent:?} must be visible in {viewport:?}"
    );
    cx.simulate_keystrokes("right");
    frames(cx);
    assert!(
        cx.debug_bounds("commit-row-new/000.txt").is_some(),
        "Right into the first child reveals it"
    );
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn custom_space_binding_hides_the_list_gesture_hint(cx: &mut TestAppContext) {
    let _restore = RestoreKeys(crate::actions::keymap::overrides());
    crate::actions::keymap::save(&std::collections::BTreeMap::from([(
        Named::Copy,
        "space".into(),
    )]))
    .unwrap();
    let (_dir, root, cx) = mount(cx);
    cx.update(|w, cx| root.update(cx, |r, cx| aim(r, Path::new("a.txt"), w, cx)));
    frames(cx);
    assert!(
        cx.debug_bounds("popup-note-In the list, Space toggles commit inclusion.")
            .is_none(),
        "Space cannot advertise inclusion when it is rebound"
    );
    assert!(
        cx.debug_bounds("popup-shortcut-Space").is_some(),
        "the effective Copy binding is still displayed"
    );
    cx.simulate_keystrokes("escape");
    frames(cx);
    cx.update(|w, cx| {
        assert!(
            root.read(cx).changes().unwrap().focus.is_focused(w),
            "closing the Commit menu restores list focus"
        )
    });
    cx.simulate_keystrokes("space");
    frames(cx);
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("a.txt\nb.txt")
        );
        assert_eq!(
            root.read(cx).changes().unwrap().ticked.len(),
            2,
            "Space bound to Copy cannot toggle inclusion"
        );
    });
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn changed_directory_eligibility_refuses_frozen_include(cx: &mut TestAppContext) {
    let (dir, root, cx) = mount(cx);
    std::fs::create_dir(dir.path().join("new")).unwrap();
    std::fs::write(dir.path().join("new/one.txt"), "one").unwrap();
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.git_status.insert(
                project,
                Ok(StatusMap::from_entries(vec![StatusEntry {
                    path: "new".into(),
                    kind: Kind::Untracked,
                    staged: false,
                    unstaged: true,
                    is_dir: true,
                }])),
            );
            r.refresh_changes(project, cx);
        })
    });
    frames(cx);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            aim(r, Path::new("new"), w, cx);
            highlight(r, Deed::Change(ChangeDeed::Include), cx);
        })
    });
    std::fs::write(dir.path().join("new/two.txt"), "two").unwrap();
    cx.update(|_, cx| root.update(cx, |r, cx| r.refresh_changes(r.active_project.unwrap(), cx)));
    frames(cx);
    cx.simulate_keystrokes("enter");
    frames(cx);
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert!(
            r.changes().unwrap().ticked.is_empty(),
            "a directory menu must not quietly include newly discovered files"
        );
        assert!(
            r.notices
                .visible()
                .any(|(_, n)| n.text.contains("Selection changed"))
        );
    });
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_stale_pointer_target_cannot_open_a_menu_for_another_group(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let state = r.changes_mut().unwrap();
            state
                .selection
                .click(Path::new("a.txt"), &state.visible_paths(), false, false);
            r.open_menu(
                Target::Change {
                    path: "removed.txt".into(),
                    ticked: false,
                    unmerged: false,
                    is_dir: false,
                    unversioned: true,
                    gone: false,
                },
                Anchor::Pointer(gpui::point(px(360.), px(100.))),
                w,
                cx,
            );
            assert!(
                r.context_menu.is_none(),
                "an expired right-click must not reuse a different current selection"
            );
            assert!(
                r.notices
                    .visible()
                    .any(|(_, n)| n.text == r.ui().text(Text::CommitSelectionChanged))
            );
        })
    });
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn empty_trash_action_gives_independent_russian_feedback(cx: &mut TestAppContext) {
    let _restore = RestoreKeys(crate::actions::keymap::overrides());
    crate::actions::keymap::save(&std::collections::BTreeMap::from([(
        Named::DeleteFile,
        "ctrl-alt-t".into(),
    )]))
    .unwrap();
    let (_dir, root, cx) = mount(cx);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.settings.language = crate::i18n::Language::Russian;
            r.notices = Default::default();
            let state = r.changes_mut().unwrap();
            let rows = state.visible_paths();
            state
                .selection
                .click(Path::new("a.txt"), &rows, false, false);
            // A Command-click takes the row out and leaves the cursor on it.
            state
                .selection
                .click(Path::new("a.txt"), &rows, true, false);
            assert!(state.selection.selected.is_empty() && state.selection.focus.is_some());
            w.focus(&state.focus, cx);
        })
    });
    cx.simulate_keystrokes("ctrl-alt-t");
    frames(cx);
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            assert!(
                r.notices
                    .visible()
                    .any(|(_, n)| n.text == r.ui().text(Text::ChangesSelectRow))
            );
            assert!(!r.asking);
        })
    });
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn an_entirely_untracked_project_root_expands_includes_and_refreshes(cx: &mut TestAppContext) {
    let (dir, root, cx) = mount_project(cx, true);
    let project_path = dir.path().join("project-name");
    std::fs::create_dir_all(project_path.join("nested/deeper")).unwrap();
    std::fs::write(project_path.join("nested/deeper/child.txt"), "child").unwrap();
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let entries =
                crate::git::status::parse(&crate::git::status(&project_path).unwrap()).unwrap();
            let (inside, _) = crate::git::status::within(entries, Path::new("project-name"));
            r.git_status
                .insert(project, Ok(StatusMap::from_entries(inside)));
            r.refresh_changes(project, cx);
            w.focus(&r.changes().unwrap().focus, cx);
        })
    });
    frames(cx);
    assert!(
        cx.debug_bounds("commit-name-project-name").is_some(),
        "the root has a human-readable name"
    );
    cx.update(|w, cx| root.update(cx, |r, cx| {
        aim(r, Path::new(""), w, cx);
        let entries = r.entries_for_open(r.context_menu.as_ref().unwrap(), cx);
        assert!(!entries.iter().any(|e| matches!(e, Entry::NamedItem(_, Deed::Change(ChangeDeed::Delete)))));
        assert!(entries.iter().any(|e| matches!(e, Entry::Choice(choice) if !choice.enabled && choice.detail == Some(Label::Text(Text::CommitTrashRoot)))));
        r.context_menu = None;
        r.changes_trash_selected(w, cx);
        assert!(!r.asking && r.notices.visible().any(|(_, n)| n.text == r.ui().text(Text::CommitTrashRoot)));
        r.changes_include_selected(Some(true), cx);
        assert_eq!(r.changes().unwrap().ticked.len(), 3);
        w.focus(&r.changes().unwrap().focus, cx);
    }));
    cx.simulate_keystrokes("right");
    frames(cx);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            assert!(
                r.changes()
                    .unwrap()
                    .visible_paths()
                    .contains(&"nested".into())
            );
            r.changes_context(Path::new("nested"), w, cx);
        })
    });
    cx.simulate_keystrokes("right");
    frames(cx);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.changes_context(Path::new("nested/deeper"), w, cx)
        })
    });
    cx.simulate_keystrokes("right");
    frames(cx);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.changes_context(Path::new("nested/deeper/child.txt"), w, cx);
            let project = r.active_project.unwrap();
            r.refresh_changes(project, cx);
            // A selected discovered child remains present during refresh, before completion.
            assert!(
                r.changes()
                    .unwrap()
                    .selection
                    .contains(Path::new("nested/deeper/child.txt"))
            );
        })
    });
    frames(cx);
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            assert!(
                r.changes()
                    .unwrap()
                    .selection
                    .contains(Path::new("nested/deeper/child.txt"))
            );
            assert_eq!(r.changes().unwrap().ticked.len(), 3);
        })
    });
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.status_workers.insert(
                project,
                crate::root::git::StatusWorker::start(project_path.clone()),
            );
            r.changes_context(Path::new(""), w, cx);
            r.add_to_vcs_selected(cx); // actual immediate Add action resolves the root to explicit files
            r.changes_context(Path::new("nested/deeper/child.txt"), w, cx);
        })
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let staged = cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.poll_git(cx);
                r.git_status
                    .get(&r.active_project.unwrap())
                    .and_then(|map| map.as_ref().ok())
                    .and_then(|map| map.entry(Path::new("nested/deeper/child.txt")))
                    .is_some_and(|e| e.staged)
            })
        });
        if staged {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "immediate staging must finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    frames(cx);
    assert_eq!(
        crate::git::run(&project_path, &["ls-files", "-z"]).unwrap(),
        b"a.txt\0b.txt\0nested/deeper/child.txt\0"
    );
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            let state = r.changes().unwrap();
            assert_eq!(state.list.len(), 3);
            assert!(
                !state.list.iter().any(|l| l.row.path.as_os_str().is_empty()),
                "no directory roots: no stale subtree is preserved"
            );
            assert!(
                state
                    .selection
                    .contains(Path::new("nested/deeper/child.txt"))
            );
        })
    });
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn prepared_stage_refusal_preserves_editor_message(cx: &mut TestAppContext) {
    prepared_refusal_preserves_editor_message(cx, true);
}
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn prepared_unstage_refusal_preserves_editor_message(cx: &mut TestAppContext) {
    prepared_refusal_preserves_editor_message(cx, false);
}
fn prepared_refusal_preserves_editor_message(cx: &mut TestAppContext, fail_stage: bool) {
    let (dir, root, cx) = mount(cx);
    crate::git::run(
        dir.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=t@example.invalid",
            "commit",
            "--allow-empty",
            "-qm",
            "base",
        ],
    )
    .unwrap();
    crate::git::write(dir.path(), &crate::git::Write::Stage(vec!["b.txt".into()])).unwrap();
    let head = crate::git::run(dir.path(), &["rev-parse", "HEAD"]).unwrap();
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let mut entries = vec![StatusEntry {
                path: "b.txt".into(),
                kind: Kind::Added,
                staged: true,
                unstaged: false,
                is_dir: false,
            }];
            entries.push(StatusEntry {
                path: "missing.txt".into(),
                kind: if fail_stage {
                    Kind::Untracked
                } else {
                    Kind::Modified
                },
                staged: !fail_stage,
                unstaged: fail_stage,
                is_dir: false,
            });
            if !fail_stage {
                entries.push(StatusEntry {
                    path: "a.txt".into(),
                    kind: Kind::Untracked,
                    staged: false,
                    unstaged: true,
                    is_dir: false,
                });
            }
            r.git_status
                .insert(project, Ok(StatusMap::from_entries(entries)));
            r.refresh_changes(project, cx);
            r.changes_mut().unwrap().ticked =
                std::collections::HashSet::from([
                    if fail_stage { "missing.txt" } else { "a.txt" }.into()
                ]);
            let message = r.changes().unwrap().message.clone().unwrap();
            message.update(cx, |m, cx| {
                m.set_value("Keep this message\n\n# detail", w, cx)
            });
            r.status_workers.insert(
                project,
                crate::root::git::StatusWorker::start(dir.path().to_path_buf()),
            );
            r.notices = Default::default();
            r.commit(cx);
        })
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let failed = cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.poll_git(cx);
                r.notices
                    .visible()
                    .any(|(_, n)| n.text.contains("missing.txt"))
            })
        });
        if failed {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Git refusal must reach the window"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    frames(cx);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.ensure_message_box(w, cx);
            assert_eq!(
                r.changes()
                    .unwrap()
                    .message
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .value()
                    .as_str(),
                "Keep this message\n\n# detail"
            );
            let project = r.active_project.unwrap();
            assert!(
                r.consoles[&project]
                    .lines()
                    .any(|line| line.failed && line.said.contains("missing.txt"))
            );
            assert!(
                !r.consoles[&project]
                    .lines()
                    .any(|line| line.argv.first().is_some_and(|arg| arg == "commit"))
            );
        })
    });
    assert_eq!(
        head,
        crate::git::run(dir.path(), &["rev-parse", "HEAD"]).unwrap()
    );
}

#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn project_root_clipboard_matches_keyboard_and_menu(cx: &mut TestAppContext) {
    let (dir, root, cx) = mount_project(cx, true);
    let project_path = dir.path().join("project-name");
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let (inside, _) = crate::git::status::within(
                crate::git::status::parse(&crate::git::status(&project_path).unwrap()).unwrap(),
                Path::new("project-name"),
            );
            r.git_status
                .insert(project, Ok(StatusMap::from_entries(inside)));
            r.refresh_changes(project, cx);
            r.changes_context(Path::new(""), w, cx);
        })
    });
    frames(cx);
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into())));
    cx.simulate_keystrokes("cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(".")
        )
    });
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into())));
    cx.simulate_keystrokes("cmd-shift-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(project_path.to_str().unwrap())
        )
    });
    for (deed, expected) in [
        (ChangeDeed::CopyPaths, ".".to_owned()),
        (
            ChangeDeed::CopyFullPaths,
            project_path.display().to_string(),
        ),
    ] {
        cx.update(|w, cx| {
            root.update(cx, |r, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()));
                aim(r, Path::new(""), w, cx);
                highlight(r, Deed::Change(deed), cx);
            })
        });
        cx.simulate_keystrokes("enter");
        frames(cx);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(expected.as_str())
            )
        });
    }
}

/// GLD-026, review finding `lenses-5`: a commit pressed while a hunk write
/// has no status yet waits for it, and takes only what was staged — not
/// the whole file the panel's last map would have added.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_commit_pressed_during_a_hunk_write_takes_only_the_staged_hunk(cx: &mut TestAppContext) {
    let (dir, root, cx) = mount(cx);
    let git = |args: &[&str]| crate::git::run(dir.path(), args).unwrap();
    let base: String = (1..=12).map(|i| format!("line {i}\n")).collect();
    std::fs::write(dir.path().join("a.txt"), &base).unwrap();
    git(&["add", "a.txt"]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@e.invalid",
        "commit",
        "-qm",
        "base",
    ]);
    let working = base
        .replace("line 1\n", "LINE 1\n")
        .replace("line 11\n", "LINE 11\n");
    std::fs::write(dir.path().join("a.txt"), &working).unwrap();
    let staged = base.replace("line 1\n", "LINE 1\n");
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let map = crate::git::status::parse(&crate::git::status(dir.path()).unwrap()).unwrap();
            r.git_status
                .insert(project, Ok(StatusMap::from_entries(map)));
            r.refresh_changes(project, cx);
            r.changes_mut().unwrap().ticked = std::collections::HashSet::from(["a.txt".into()]);
            let message = r.changes().unwrap().message.clone().unwrap();
            message.update(cx, |m, cx| m.set_value("Only the first", w, cx));
            r.status_workers.insert(
                project,
                crate::root::git::StatusWorker::start(dir.path().to_path_buf()),
            );
            r.send_hunk(
                project,
                crate::git::Write::StageHunk(crate::git::Hunk {
                    path: "a.txt".into(),
                    preimage: std::sync::Arc::from(base.as_bytes()),
                    postimage: std::sync::Arc::from(staged.as_bytes()),
                    disk: Some(std::sync::Arc::from(working.as_bytes())),
                    head: std::sync::Arc::from(base.as_bytes()),
                }),
                cx,
            );
            r.commit(cx);
        })
    });
    let before = String::from_utf8(git(&["rev-parse", "HEAD"])).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        cx.update(|_, cx| root.update(cx, |r, cx| r.poll_git(cx)));
        if String::from_utf8(git(&["rev-parse", "HEAD"])).unwrap() != before {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "the commit must run");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        String::from_utf8(git(&["show", "HEAD:a.txt"])).unwrap(),
        staged
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        working
    );
}

/// GLD-026: a partial row counts as in the commit for its group's head, as
/// on the approved board; the head's click clears the group, and the next
/// ticks every row whole.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_partial_row_counts_as_included_for_its_group(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.git_status.insert(
                project,
                Ok(StatusMap::from_entries(vec![StatusEntry {
                    path: "a.txt".into(),
                    kind: Kind::Modified,
                    staged: true,
                    unstaged: true,
                    is_dir: false,
                }])),
            );
            r.refresh_changes(project, cx);
            // Partial as a hunk write leaves it (review r3: an arrival alone
            // ticks it whole, as decision 16 always did).
            let state = r.changes_mut().unwrap();
            state.ticked.clear();
            state.partial = std::collections::HashSet::from(["a.txt".into()]);
            r.changes_toggle_group(changes::Section::Changes, cx);
            let state = r.changes().unwrap();
            assert!(state.partial.is_empty() && state.ticked.is_empty());
            r.changes_toggle_group(changes::Section::Changes, cx);
            assert!(r.changes().unwrap().ticked.contains(Path::new("a.txt")));
        })
    });
}

/// Stage a hunk of `a.txt` and press Commit before its status lands; the
/// fixture's write is built from `disk`, so a disk that moved meanwhile makes
/// the write refuse. Returns the repository, HEAD before, the root.
fn commit_during_a_hunk_write(
    cx: &mut TestAppContext,
    disk_moves: bool,
    leave_project: bool,
) -> (
    tempfile::TempDir,
    String,
    Entity<Root>,
    &mut VisualTestContext,
) {
    let (dir, root, cx) = mount(cx);
    let git = |args: &[&str]| crate::git::run(dir.path(), args).unwrap();
    let base: String = (1..=12).map(|i| format!("line {i}\n")).collect();
    std::fs::write(dir.path().join("a.txt"), &base).unwrap();
    git(&["add", "a.txt"]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@e.invalid",
        "commit",
        "-qm",
        "base",
    ]);
    let working = base
        .replace("line 1\n", "LINE 1\n")
        .replace("line 11\n", "LINE 11\n");
    std::fs::write(dir.path().join("a.txt"), &working).unwrap();
    let staged = base.replace("line 1\n", "LINE 1\n");
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let map = crate::git::status::parse(&crate::git::status(dir.path()).unwrap()).unwrap();
            r.git_status
                .insert(project, Ok(StatusMap::from_entries(map)));
            r.refresh_changes(project, cx);
            r.changes_mut().unwrap().ticked = std::collections::HashSet::from(["a.txt".into()]);
            let message = r.changes().unwrap().message.clone().unwrap();
            message.update(cx, |m, cx| m.set_value("Only the first", w, cx));
            r.status_workers.insert(
                project,
                crate::root::git::StatusWorker::start(dir.path().to_path_buf()),
            );
            if disk_moves {
                std::fs::write(dir.path().join("a.txt"), format!("{working}more\n")).unwrap();
            }
            r.notices = Default::default();
            r.send_hunk(
                project,
                crate::git::Write::StageHunk(crate::git::Hunk {
                    path: "a.txt".into(),
                    preimage: std::sync::Arc::from(base.as_bytes()),
                    postimage: std::sync::Arc::from(staged.as_bytes()),
                    disk: Some(std::sync::Arc::from(working.as_bytes())),
                    head: std::sync::Arc::from(base.as_bytes()),
                }),
                cx,
            );
            r.commit(cx);
            if leave_project {
                r.active_project = None;
            }
        })
    });
    let before = String::from_utf8(git(&["rev-parse", "HEAD"])).unwrap();
    (dir, before, root, cx)
}

/// Review r1 `bugs+impl-3`, the person's choice: a hunk write that refuses
/// cancels the commit waiting for it, says so, and keeps the message.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_refused_hunk_write_cancels_the_commit_waiting_for_it(cx: &mut TestAppContext) {
    let (dir, before, root, cx) = commit_during_a_hunk_write(cx, true, false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let settled = cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.poll_git(cx);
                r.notices
                    .visible()
                    .any(|(_, n)| n.text.contains("Commit not made"))
            })
        });
        if settled {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no cancellation notice"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // A moment more for a commit that should not come.
    for _ in 0..20 {
        cx.update(|_, cx| root.update(cx, |r, cx| r.poll_git(cx)));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        String::from_utf8(crate::git::run(dir.path(), &["rev-parse", "HEAD"]).unwrap()).unwrap(),
        before,
        "no commit"
    );
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.ensure_message_box(w, cx);
            let message = r.changes().unwrap().message.clone().unwrap();
            assert_eq!(message.read(cx).value().as_str(), "Only the first");
        })
    });
}

/// Review r1 `bugs+impl-9`: a commit waiting for a hunk write runs in its
/// own project, whichever project is in front when the status lands.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_waiting_commit_runs_in_its_project_after_the_front_moves(cx: &mut TestAppContext) {
    let (dir, before, root, cx) = commit_during_a_hunk_write(cx, false, true);
    let git = |args: &[&str]| crate::git::run(dir.path(), args).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        cx.update(|_, cx| root.update(cx, |r, cx| r.poll_git(cx)));
        if String::from_utf8(git(&["rev-parse", "HEAD"])).unwrap() != before {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "the commit must run");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let base: String = (1..=12).map(|i| format!("line {i}\n")).collect();
    assert_eq!(
        String::from_utf8(git(&["show", "HEAD:a.txt"])).unwrap(),
        base.replace("line 1\n", "LINE 1\n")
    );
}

/// Review r1 `docs+tests-7`: an Unstage written while a commit landed is
/// said in a notice.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn an_unstage_racing_a_commit_is_said(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.notices = Default::default();
            let write = crate::git::Write::UnstageHunk(crate::git::Hunk {
                path: "a.txt".into(),
                preimage: std::sync::Arc::from(&b"x"[..]),
                postimage: std::sync::Arc::from(&b"y"[..]),
                disk: None,
                head: std::sync::Arc::from(&b"y"[..]),
            });
            r.hunks_landed(
                project,
                vec![
                    crate::root::git::HunkEvent::Outcome(
                        write,
                        crate::root::git::HunkOutcome::HeadMoved,
                    ),
                    crate::root::git::HunkEvent::Map(true),
                ],
                cx,
            );
            assert!(r.notices.visible().any(|(_, n)| {
                n.text
                    .contains("A commit landed while the change was unstaged")
            }));
        })
    });
}

/// A repository with `a.txt` committed as twelve lines, the working copy
/// and the index as given, its map in the panel with `a.txt` ticked full,
/// a real git thread, and the message set. Returns HEAD before.
fn staged_fixture<'a>(
    cx: &'a mut TestAppContext,
    index: &str,
    working: &str,
) -> (
    tempfile::TempDir,
    String,
    Entity<Root>,
    &'a mut VisualTestContext,
) {
    let (dir, root, cx) = mount(cx);
    let git = |args: &[&str]| crate::git::run(dir.path(), args).unwrap();
    std::fs::write(dir.path().join("a.txt"), twelve()).unwrap();
    git(&["add", "a.txt"]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@e.invalid",
        "commit",
        "-qm",
        "base",
    ]);
    std::fs::write(dir.path().join("a.txt"), index).unwrap();
    git(&["add", "a.txt"]);
    std::fs::write(dir.path().join("a.txt"), working).unwrap();
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let map = crate::git::status::parse(&crate::git::status(dir.path()).unwrap()).unwrap();
            r.git_status
                .insert(project, Ok(StatusMap::from_entries(map)));
            r.refresh_changes(project, cx);
            let state = r.changes_mut().unwrap();
            state.partial.clear();
            state.ticked = std::collections::HashSet::from(["a.txt".into()]);
            let message = r.changes().unwrap().message.clone().unwrap();
            message.update(cx, |m, cx| m.set_value("Held", w, cx));
            r.status_workers.insert(
                project,
                crate::root::git::StatusWorker::start(dir.path().to_path_buf()),
            );
            r.notices = Default::default();
        })
    });
    let before = String::from_utf8(git(&["rev-parse", "HEAD"])).unwrap();
    (dir, before, root, cx)
}

fn twelve() -> String {
    (1..=12).map(|i| format!("line {i}\n")).collect()
}

fn hunk(pre: &str, post: &str, head: &str) -> crate::git::Hunk {
    crate::git::Hunk {
        path: "a.txt".into(),
        preimage: std::sync::Arc::from(pre.as_bytes()),
        postimage: std::sync::Arc::from(post.as_bytes()),
        disk: None,
        head: std::sync::Arc::from(head.as_bytes()),
    }
}

/// Poll until `done`, at most 20 s.
fn poll_until(
    root: &Entity<Root>,
    cx: &mut VisualTestContext,
    mut done: impl FnMut(&mut Root) -> bool,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.poll_git(cx);
                done(r)
            })
        }) {
            return;
        }
        assert!(std::time::Instant::now() < deadline, "timed out");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Review r2 `arch+quality-7`: unstaging the last staged hunk of a fully
/// ticked file and pressing Commit at once does not stage it back.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_waiting_commit_does_not_restage_a_hunk_just_unstaged(cx: &mut TestAppContext) {
    let index = twelve().replace("line 1\n", "LINE 1\n");
    let (dir, before, root, cx) = staged_fixture(cx, &index, &index);
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.send_hunk(
                project,
                crate::git::Write::UnstageHunk(hunk(&index, &twelve(), &twelve())),
                cx,
            );
            r.commit(cx);
        })
    });
    poll_until(&root, cx, |r| {
        r.changes()
            .is_some_and(|s| !s.ticked.contains(Path::new("a.txt")))
    });
    for _ in 0..30 {
        cx.update(|_, cx| root.update(cx, |r, cx| r.poll_git(cx)));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let git =
        |args: &[&str]| String::from_utf8(crate::git::run(dir.path(), args).unwrap()).unwrap();
    assert_eq!(git(&["rev-parse", "HEAD"]), before, "nothing was committed");
    assert_eq!(
        git(&["show", ":a.txt"]),
        twelve(),
        "the hunk stays unstaged"
    );
}

/// Review r2 `adversarial-3`: a file staying partial after a hunk write is
/// committed as its index stands, though its tick was full.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_waiting_commit_takes_only_the_index_of_a_file_that_stays_partial(cx: &mut TestAppContext) {
    let index = twelve().replace("line 1\n", "LINE 1\n");
    let working = index
        .replace("line 5\n", "LINE 5\n")
        .replace("line 10\n", "LINE 10\n");
    let staged = index.replace("line 5\n", "LINE 5\n");
    let (dir, before, root, cx) = staged_fixture(cx, &index, &working);
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.send_hunk(
                project,
                crate::git::Write::StageHunk(hunk(&index, &staged, &twelve())),
                cx,
            );
            r.commit(cx);
        })
    });
    let git =
        |args: &[&str]| String::from_utf8(crate::git::run(dir.path(), args).unwrap()).unwrap();
    poll_until(&root, cx, |_| git(&["rev-parse", "HEAD"]) != before);
    assert_eq!(git(&["show", "HEAD:a.txt"]), staged);
}

/// Review r2 `bugs+impl-1`: the refusal is answered before Commit is
/// pressed and its map after; the commit is still cancelled.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_refusal_answered_before_commit_still_cancels_it(cx: &mut TestAppContext) {
    let (dir, before, root, cx) =
        staged_fixture(cx, &twelve(), &twelve().replace("line 1\n", "X\n"));
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.hunk_in_flight_for_test(project);
            let write = crate::git::Write::StageHunk(hunk("x", "y", "x"));
            r.hunks_landed(
                project,
                vec![crate::root::git::HunkEvent::Outcome(
                    write,
                    crate::root::git::HunkOutcome::Stale,
                )],
                cx,
            );
            r.commit(cx);
            r.hunks_landed(project, vec![crate::root::git::HunkEvent::Map(true)], cx);
            assert!(
                r.notices
                    .visible()
                    .any(|(_, n)| n.text.contains("Commit not made"))
            );
        })
    });
    for _ in 0..30 {
        cx.update(|_, cx| root.update(cx, |r, cx| r.poll_git(cx)));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        String::from_utf8(crate::git::run(dir.path(), &["rev-parse", "HEAD"]).unwrap()).unwrap(),
        before
    );
}

/// Review r2 `arch+quality-9`: a git thread replaced while a hunk write is
/// unanswered takes its flight with it; a commit waiting for it is
/// cancelled and said, and the next one is not held.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_replaced_git_thread_cancels_the_commit_waiting_on_it(cx: &mut TestAppContext) {
    let (dir, _before, root, cx) = staged_fixture(cx, &twelve(), &twelve());
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.hunk_in_flight_for_test(project);
            r.commit(cx);
            r.status_workers.insert(
                project,
                crate::root::git::StatusWorker::start(dir.path().to_path_buf()),
            );
            r.poll_git(cx);
            assert!(
                r.notices
                    .visible()
                    .any(|(_, n)| n.text.contains("Commit not made"))
            );
            assert!(!r.hold_commit_for_hunks(project));
        })
    });
}

/// Review r2 `docs+tests-11`: the worker's own check after the write sees a
/// commit landing between an Unstage's checks and its write, and the window
/// says so through its ordinary polling.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_commit_landing_during_an_unstage_is_seen_by_the_worker(cx: &mut TestAppContext) {
    let index = twelve().replace("line 1\n", "LINE 1\n");
    let (dir, _before, root, cx) = staged_fixture(cx, &index, &index);
    let repo = dir.path().to_path_buf();
    *crate::root::git::BETWEEN_CHECKS_AND_WRITE.lock().unwrap() = Some((
        repo.clone(),
        Box::new(move || {
            crate::git::run(
                &repo,
                &[
                    "-c",
                    "user.name=T",
                    "-c",
                    "user.email=t@e.invalid",
                    "commit",
                    "-qm",
                    "outside",
                ],
            )
            .unwrap();
        }),
    ));
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.send_hunk(
                project,
                crate::git::Write::UnstageHunk(hunk(&index, &twelve(), &twelve())),
                cx,
            );
            // Review r3 `arch+quality-3`: a commit held for it.
            r.commit(cx);
        })
    });
    poll_until(&root, cx, |r| {
        r.notices.visible().any(|(_, n)| {
            n.text
                .contains("A commit landed while the change was unstaged")
        })
    });
    for _ in 0..30 {
        cx.update(|_, cx| root.update(cx, |r, cx| r.poll_git(cx)));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    *crate::root::git::BETWEEN_CHECKS_AND_WRITE.lock().unwrap() = None;
    let subject =
        String::from_utf8(crate::git::run(dir.path(), &["log", "-1", "--format=%s"]).unwrap())
            .unwrap();
    assert_eq!(
        subject.trim(),
        "outside",
        "the held commit did not record the inverse"
    );
    assert!(cx.update(|_, cx| {
        root.read(cx)
            .notices
            .visible()
            .any(|(_, n)| n.text.contains("Commit not made"))
    }));
    assert_eq!(
        String::from_utf8(crate::git::run(dir.path(), &["show", ":a.txt"]).unwrap()).unwrap(),
        twelve(),
        "the write landed"
    );
}

/// Review r2 `bugs+impl-12`: a refused write asks the diff showing its path
/// again, so the control that offered it does not stay enabled.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_refusal_refreshes_the_diff_that_offered_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let working = twelve().replace("line 1\n", "LINE 1\n");
    let (dir, _before, root, cx) = staged_fixture(cx, &twelve(), &working);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.open_diff(Some("a.txt".into()), guild_session::Base::Head, w, cx)
        })
    });
    let diff = cx.update(|_, cx| {
        root.read(cx)
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|b| match b {
                crate::Body::Diff { tab, .. } => Some(tab.clone()),
                _ => None,
            })
            .unwrap()
    });
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        let offer = cx.update(|_, cx| {
            diff.update(cx, |d, cx| {
                d.poll(cx);
                d.hunk_offer(0, 0)
            })
        });
        if offer.is_some_and(|(_, available)| available.is_ok()) {
            break;
        }
        assert!(std::time::Instant::now() < until, "no offer");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::fs::write(dir.path().join(".gitattributes"), "a.txt ident\n").unwrap();
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let write = crate::git::Write::StageHunk(hunk("x", "y", "x"));
            r.hunks_landed(
                project,
                vec![
                    crate::root::git::HunkEvent::Outcome(
                        write,
                        crate::root::git::HunkOutcome::Converted,
                    ),
                    crate::root::git::HunkEvent::Map(true),
                ],
                cx,
            );
        })
    });
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        let offer = cx.update(|_, cx| {
            diff.update(cx, |d, cx| {
                d.poll(cx);
                d.hunk_offer(0, 0)
            })
        });
        if offer.is_some_and(|(_, available)| available.is_err()) {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "the control stayed enabled"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Review r3 `bugs+impl-2`: the status read after a hunk write fails. The
/// held commit is cancelled and said, the ticks stay as the person left
/// them, and the next usable map ticks the written row from the index.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_failed_status_read_cancels_the_held_commit_and_keeps_the_ticks(cx: &mut TestAppContext) {
    let index = twelve().replace("line 1\n", "LINE 1\n");
    let working = index.replace("line 9\n", "LINE 9\n");
    let (dir, _, root, cx) = staged_fixture(cx, &index, &working);
    let git = |args: &[&str]| crate::git::run(dir.path(), args).unwrap();
    std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
    git(&["add", "b.txt"]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@e.invalid",
        "commit",
        "-qm",
        "b",
        "--",
        "b.txt",
    ]);
    std::fs::write(dir.path().join("b.txt"), "B\n").unwrap();
    let before = String::from_utf8(git(&["rev-parse", "HEAD"])).unwrap();
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let map = crate::git::status::parse(&crate::git::status(dir.path()).unwrap()).unwrap();
            r.git_status
                .insert(project, Ok(StatusMap::from_entries(map)));
            r.refresh_changes(project, cx);
            let state = r.changes_mut().unwrap();
            state.ticked = std::collections::HashSet::from(["a.txt".into()]);
            state.partial.clear();
            r.hunk_in_flight_for_test(project);
            r.commit(cx);
            let write = crate::git::Write::StageHunk(hunk(&index, &working, &twelve()));
            r.git_status.insert(
                project,
                Err(crate::git::GitError::Failed("index file corrupt".into())),
            );
            r.refresh_changes(project, cx);
            r.hunks_landed(
                project,
                vec![
                    crate::root::git::HunkEvent::Outcome(
                        write,
                        crate::root::git::HunkOutcome::Written,
                    ),
                    crate::root::git::HunkEvent::Map(false),
                ],
                cx,
            );
            assert!(
                r.notices
                    .visible()
                    .any(|(_, n)| n.text.contains("Commit not made")),
                "cancelled and said"
            );
            assert_eq!(
                r.changes().unwrap().ticked,
                std::collections::HashSet::from(["a.txt".into()]),
                "a.txt keeps its tick"
            );
            // The next usable map: a.txt follows the index, and b.txt is
            // the row it was — not ticked as new.
            std::fs::write(dir.path().join("a.txt"), &working).unwrap();
            crate::git::run(dir.path(), &["add", "a.txt"]).unwrap();
            let map = crate::git::status::parse(&crate::git::status(dir.path()).unwrap()).unwrap();
            r.git_status
                .insert(project, Ok(StatusMap::from_entries(map)));
            r.refresh_changes(project, cx);
            r.hunks_landed(project, vec![crate::root::git::HunkEvent::Map(true)], cx);
            let state = r.changes().unwrap();
            assert!(state.ticked.contains(Path::new("a.txt")));
            assert!(!state.ticked.contains(Path::new("b.txt")));
            assert!(!r.hold_commit_for_hunks(project), "settled");
        })
    });
    for _ in 0..30 {
        cx.update(|_, cx| root.update(cx, |r, cx| r.poll_git(cx)));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        String::from_utf8(crate::git::run(dir.path(), &["rev-parse", "HEAD"]).unwrap()).unwrap(),
        before,
        "nothing was committed"
    );
}

/// Review r3 `docs+tests-4`: a file a diff shows from the disk becomes a
/// FIFO with no writer. Neither opening the diff nor the watcher's batch
/// blocks the window's thread on it.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_fifo_in_place_of_a_shown_file_does_not_block_the_window(cx: &mut TestAppContext) {
    use std::os::unix::fs::OpenOptionsExt as _;
    cx.executor().allow_parking();
    let (dir, _before, root, cx) =
        staged_fixture(cx, &twelve(), &twelve().replace("line 1\n", "X\n"));
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.open_diff(Some("a.txt".into()), guild_session::Base::Head, w, cx)
        })
    });
    std::fs::remove_file(dir.path().join("a.txt")).unwrap();
    let fifo = std::ffi::CString::new(dir.path().join("a.txt").to_str().unwrap()).unwrap();
    // SAFETY: a valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0);
    let (done, wait) = std::sync::mpsc::channel();
    let path = dir.path().to_path_buf();
    // Should the window block, a writer is what frees it — and the test fails.
    let rescue = std::thread::spawn(move || {
        if wait.recv_timeout(std::time::Duration::from_secs(5)).is_ok() {
            return true;
        }
        // Free every blocked read until the window is through.
        while wait
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err()
        {
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(path.join("a.txt"));
        }
        false
    });
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let mut batch = crate::watch::Batch::new(&ignore::gitignore::Gitignore::empty());
            batch.push(Path::new("a.txt"));
            r.apply_batch_to_diffs(project, &batch, cx);
            r.open_diff(Some("a.txt".into()), guild_session::Base::Unstaged, w, cx);
        })
    });
    let _ = done.send(());
    assert!(rescue.join().unwrap(), "the window blocked on the FIFO");
}

/// Review r3 `arch+quality-5`: the window takes one write per file while it
/// is unanswered; a second for the same file is not queued.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_second_write_for_a_file_in_flight_is_not_queued(cx: &mut TestAppContext) {
    let working = twelve().replace("line 1\n", "X\n");
    let (_dir, _before, root, cx) = staged_fixture(cx, &twelve(), &working);
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let write = crate::git::Write::StageHunk(hunk(&twelve(), &working, &twelve()));
            r.send_hunk(project, write.clone(), cx);
            r.send_hunk(project, write, cx);
            assert_eq!(r.hunks_outstanding_for_test(project), 1);
        })
    });
    poll_until(&root, cx, |r| {
        let project = r.active_project.unwrap();
        r.hunks_outstanding_for_test(project) == 0
    });
    cx.update(|_, cx| {
        let file = _dir.path().join("a.txt");
        assert!(!crate::root::hunks::hunk_in_flight(cx, &file), "released");
    });
}

/// Review r3 `bugs+impl-3`: nothing ticked yet, a message written, and a
/// Stage in flight. The Commit button is lit — it asks the same question
/// the button's click handler is installed by — and a click is held like
/// `cmd-enter`, to be planned from the status the write produces.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn the_commit_button_is_reachable_while_the_first_tick_is_pending(cx: &mut TestAppContext) {
    let (_dir, _before, root, cx) =
        staged_fixture(cx, &twelve(), &twelve().replace("line 1\n", "X\n"));
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.changes_mut().unwrap().ticked.clear();
            assert_eq!(
                r.commit_blocked_by(cx),
                Some(crate::i18n::Text::CommitNothingStaged),
                "nothing ticked, nothing pending"
            );
            r.hunk_in_flight_for_test(project);
            assert_eq!(r.commit_blocked_by(cx), None, "the button is lit");
            r.commit(cx);
            assert!(
                !r.notices
                    .visible()
                    .any(|(_, n)| n.text.contains("Nothing is ticked")),
                "held, not refused"
            );
        })
    });
}

/// Review r4 `bugs+impl-1`: a hunk write's answer, the status read after it
/// failing, and a usable map behind that, all drained in one poll. The held
/// commit is cancelled — the failure is not hidden by the map after it —
/// and the usable map still settles the tick.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_failed_map_drained_with_a_usable_one_still_cancels_the_held_commit(cx: &mut TestAppContext) {
    let working = twelve().replace("line 1\n", "X\n");
    let (dir, before, root, cx) = staged_fixture(cx, &twelve(), &working);
    let (worker, answers, _asks) = crate::root::git::StatusWorker::fake(dir.path());
    let staged = twelve().replace("line 1\n", "X\n");
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.status_workers.insert(project, worker);
            r.send_hunk(
                project,
                crate::git::Write::StageHunk(hunk(&twelve(), &staged, &twelve())),
                cx,
            );
            r.commit(cx);
            r.notices = Default::default();
        })
    });
    // What the git thread would have done: the write, then two walks.
    crate::git::run(dir.path(), &["add", "a.txt"]).unwrap();
    let map = crate::git::status::parse(&crate::git::status(dir.path()).unwrap()).unwrap();
    answers
        .send(crate::root::git::Answer::Hunk(
            crate::git::Write::StageHunk(hunk(&twelve(), &staged, &twelve())),
            crate::root::git::HunkOutcome::Written,
        ))
        .unwrap();
    answers
        .send(crate::root::git::Answer::Map(Err(
            crate::git::GitError::Failed("index file corrupt".into()),
        )))
        .unwrap();
    answers
        .send(crate::root::git::Answer::Map(Ok(StatusMap::from_entries(
            map,
        ))))
        .unwrap();
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.poll_git(cx);
            assert!(
                r.notices
                    .visible()
                    .any(|(_, n)| n.text.contains("Commit not made")),
                "cancelled and said"
            );
            assert!(r.changes().unwrap().ticked.contains(Path::new("a.txt")));
        })
    });
    assert_eq!(
        String::from_utf8(crate::git::run(dir.path(), &["rev-parse", "HEAD"]).unwrap()).unwrap(),
        before,
        "nothing was committed"
    );
}

/// Review r4 `bugs+impl-3`: `apply` swapped the entry in and the read-back
/// could not answer. The held commit is cancelled, the notice says the
/// write may have happened, and the tick follows the index once a usable
/// map lands — a retry cannot take the untouched block.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn an_unverified_write_cancels_the_commit_and_ticks_from_the_index(cx: &mut TestAppContext) {
    let working = twelve()
        .replace("line 1\n", "X\n")
        .replace("line 9\n", "Y\n");
    let staged = twelve().replace("line 1\n", "X\n");
    let (dir, before, root, cx) = staged_fixture(cx, &twelve(), &working);
    let (worker, answers, _asks) = crate::root::git::StatusWorker::fake(dir.path());
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.status_workers.insert(project, worker);
            r.send_hunk(
                project,
                crate::git::Write::StageHunk(hunk(&twelve(), &staged, &twelve())),
                cx,
            );
            r.commit(cx);
            r.notices = Default::default();
        })
    });
    std::fs::write(dir.path().join("a.txt"), &staged).unwrap();
    crate::git::run(dir.path(), &["add", "a.txt"]).unwrap();
    std::fs::write(dir.path().join("a.txt"), &working).unwrap();
    let map = crate::git::status::parse(&crate::git::status(dir.path()).unwrap()).unwrap();
    answers
        .send(crate::root::git::Answer::Hunk(
            crate::git::Write::StageHunk(hunk(&twelve(), &staged, &twelve())),
            crate::root::git::HunkOutcome::Unverified,
        ))
        .unwrap();
    answers
        .send(crate::root::git::Answer::Map(Ok(StatusMap::from_entries(
            map,
        ))))
        .unwrap();
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.poll_git(cx);
            assert!(
                r.notices
                    .visible()
                    .any(|(_, n)| n.text.contains("Commit not made"))
            );
            assert!(
                r.notices
                    .visible()
                    .any(|(_, n)| n.text.contains("could not be read back"))
            );
            let state = r.changes().unwrap();
            assert!(
                state.partial.contains(Path::new("a.txt")),
                "partial, from the index"
            );
            assert!(!state.ticked.contains(Path::new("a.txt")));
        })
    });
    assert_eq!(
        String::from_utf8(crate::git::run(dir.path(), &["rev-parse", "HEAD"]).unwrap()).unwrap(),
        before
    );
}

/// Review r4 `bugs+impl-6`: the Staged diff shows two git blobs. Restoring
/// one and a watcher batch naming its file read no working text on the
/// window's thread — the same rule as opening one.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn a_staged_diff_reads_no_working_text_when_restored_or_watched(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let working = twelve().replace("line 1\n", "X\n");
    let (dir, _before, root, cx) = staged_fixture(cx, &working, &working);
    let diff = cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let id = r.restore_diff_tab(
                project,
                dir.path(),
                Some("a.txt".into()),
                guild_session::Base::Index,
                cx,
            );
            r.tabs
                .iter()
                .find(|t| t.id == id)
                .into_iter()
                .flat_map(crate::Tab::bodies)
                .find_map(|b| match b {
                    crate::Body::Diff { tab, .. } => Some(tab.clone()),
                    _ => None,
                })
                .unwrap()
        })
    });
    let held = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| diff.read(cx).working_for_test(Path::new("a.txt")))
    };
    assert_eq!(held(cx), None, "restored without reading the file");
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let mut batch = crate::watch::Batch::new(&ignore::gitignore::Gitignore::empty());
            batch.push(Path::new("a.txt"));
            r.apply_batch_to_diffs(project, &batch, cx);
        })
    });
    assert_eq!(held(cx), None, "the watcher's batch read nothing");
    // The diff's own reads of git's blobs land within this test, not in the
    // next one's scheduler.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while cx.update(|_, cx| {
        diff.update(cx, |d, cx| {
            d.poll(cx);
            d.loading()
        })
    }) {
        assert!(std::time::Instant::now() < until, "the diff never loaded");
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    cx.run_until_parked();
}

/// Review r5 `docs+tests-6`: through the production git thread, `apply`
/// swaps the entry in and the read-back cannot answer. The thread's outcome
/// is unverified — the held commit is cancelled and said — and once a usable
/// map lands the file's tick follows the index: partial, so a retry cannot
/// take the untouched block.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn the_git_thread_reports_an_unverified_write_and_the_tick_turns_partial(cx: &mut TestAppContext) {
    let working = twelve()
        .replace("line 1\n", "X\n")
        .replace("line 9\n", "Y\n");
    let staged = twelve().replace("line 1\n", "X\n");
    let (dir, before, root, cx) = staged_fixture(cx, &twelve(), &working);
    let repo = dir.path().to_path_buf();
    let in_hook = repo.clone();
    *crate::git::hunk::AFTER_APPLY_IN_ROOT.lock().unwrap() = Some((
        repo.clone(),
        Box::new(move || {
            *crate::git::hunk::DEADLINE_IN_ROOT.lock().unwrap() =
                Some((in_hook.clone(), std::time::Duration::ZERO));
        }),
    ));
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.send_hunk(
                project,
                crate::git::Write::StageHunk(hunk(&twelve(), &staged, &twelve())),
                cx,
            );
            r.commit(cx);
        })
    });
    poll_until(&root, cx, |r| {
        r.notices
            .visible()
            .any(|(_, n)| n.text.contains("could not be read back"))
    });
    *crate::git::hunk::AFTER_APPLY_IN_ROOT.lock().unwrap() = None;
    *crate::git::hunk::DEADLINE_IN_ROOT.lock().unwrap() = None;
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            let project = r.active_project.unwrap();
            r.status_workers[&project].ask();
        })
    });
    poll_until(&root, cx, |r| {
        r.changes()
            .is_some_and(|s| s.partial.contains(Path::new("a.txt")))
    });
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert!(
            r.notices
                .visible()
                .any(|(_, n)| n.text.contains("Commit not made"))
        );
        assert!(!r.changes().unwrap().ticked.contains(Path::new("a.txt")));
    });
    assert_eq!(
        String::from_utf8(crate::git::run(&repo, &["rev-parse", "HEAD"]).unwrap()).unwrap(),
        before,
        "nothing was committed"
    );
}

/// The person's diff review of r5: a staged rename the person excluded,
/// another file staged by a hunk while Commit is held. The commit takes the
/// hunk's file only — neither the rename's deletion nor its addition — and
/// the rename row keeps its cleared tick.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn an_excluded_rename_stays_out_of_a_commit_held_for_a_hunk(cx: &mut TestAppContext) {
    excluded_rename_scenario(cx, false);
}

/// The same in a repository git takes for a partial clone — a remote with
/// only `partialCloneFilter`, every blob local — where the settling walk
/// detects no renames and the rename comes back as two rows.
#[gpui::test]
#[ignore = "isolated_commit_menu_corrections owns private state"]
fn an_excluded_rename_stays_out_in_a_partial_clone_too(cx: &mut TestAppContext) {
    excluded_rename_scenario(cx, true);
}

fn excluded_rename_scenario(cx: &mut TestAppContext, partial: bool) {
    let working = twelve().replace("line 1\n", "X\n");
    let (dir, before, root, cx) = staged_fixture(cx, &twelve(), &working);
    let git = |args: &[&str]| crate::git::run(dir.path(), args).unwrap();
    if partial {
        git(&["remote", "add", "origin", "https://example.invalid/r"]);
        git(&["config", "remote.origin.partialCloneFilter", "blob:none"]);
        assert!(crate::git::hunk::partial_clone(dir.path()).unwrap());
    }
    std::fs::write(dir.path().join("old.txt"), "a file that moves\n").unwrap();
    git(&["add", "old.txt"]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@e.invalid",
        "commit",
        "-qm",
        "old",
        "--",
        "old.txt",
    ]);
    git(&["mv", "old.txt", "new.txt"]);
    let base = String::from_utf8(git(&["rev-parse", "HEAD"])).unwrap();
    let _ = before;
    // The git thread's own walk shows the rename before the person acts on
    // the list it draws.
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            let project = r.active_project.unwrap();
            r.status_workers[&project].ask();
        })
    });
    poll_until(&root, cx, |r| {
        r.changes().is_some_and(|s| {
            s.list.iter().any(|l| {
                l.row.path == Path::new("new.txt")
                    && l.row.from.as_deref() == Some(Path::new("old.txt"))
            })
        })
    });
    for _ in 0..20 {
        cx.update(|_, cx| root.update(cx, |r, cx| r.poll_git(cx)));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let state = r.changes_mut().unwrap();
            // The person excludes the rename.
            state.ticked = std::collections::HashSet::from(["a.txt".into()]);
            state.partial.clear();
            r.send_hunk(
                project,
                crate::git::Write::StageHunk(hunk(&twelve(), &working, &twelve())),
                cx,
            );
            r.commit(cx);
        })
    });
    poll_until(&root, cx, |_| {
        String::from_utf8(crate::git::run(dir.path(), &["rev-parse", "HEAD"]).unwrap()).unwrap()
            != base
    });
    let changed = String::from_utf8(git(&["show", "--name-status", "--format=", "HEAD"])).unwrap();
    assert_eq!(
        changed.trim(),
        "M\ta.txt",
        "only the hunk's file was committed"
    );
    cx.update(|_, cx| {
        let state = root.read(cx).changes().unwrap();
        assert!(
            !state.ticked.contains(Path::new("old.txt")),
            "{:?}",
            state.ticked
        );
        assert!(
            !state.ticked.contains(Path::new("new.txt")),
            "{:?}",
            state.ticked
        );
    });
}
