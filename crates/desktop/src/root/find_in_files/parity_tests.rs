//! Integrated project snapshots, editor ownership and actual modifier dispatch.
use crate::{Body, Root};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use std::{path::Path, time::Duration};

// These scenarios also exercise global keymaps and real fixture-project writes.
// Keep them in an owned child even though diagnostic Root scopes its snapshots.
#[test]
fn isolated_project_search_parity() {
    let mut failures = Vec::new();
    for scenario in [
        "pending_file_searches_keep_disk_matches_and_empty_buffers_override_them",
        "existing_working_diff_follows_late_source_editor_through_save_and_close",
        "project_replace_keeps_dirty_buffers_refuses_stale_disk_and_undoes_per_file",
        "double_shift_reaches_everywhere_through_a_focused_file",
        "project_query_replacement_and_multiline_keep_their_own_input",
        "each_file_is_revalidated_after_an_earlier_file_was_applied",
        "exclude_and_cancel_preserve_unprocessed_documents",
        "watcher_refresh_waits_for_replacement_completion",
        "reinvocation_selects_the_actual_query_component",
        "late_divergent_alias_refuses_replacement",
        "late_alias_in_another_open_project_refuses_replacement",
        "excluded_identity_survives_new_display_alias",
        "alias_roster_change_during_validation_refuses_the_edit",
        "cancel_during_background_validation_leaves_buffers_unchanged",
        "single_project_replace_only_checks_selected_output",
        "multiline_tab_cycles_focus_actual_query_mask_and_replacement",
        "floating_picker_tracks_drag_over_inputs_and_final_release",
        "recent_files_is_a_project_mru_selector_without_text_input",
        "cancelled_recent_close_cannot_forget_a_later_unrelated_close",
        "titlebar_shortcuts_toggle_and_restore_focus_with_overrides",
    ] {
        let owned = tempfile::Builder::new()
            .prefix("guild-search-parity-")
            .tempdir()
            .unwrap();
        let base = owned.path().canonicalize().unwrap();
        let state = base.join("state");
        std::fs::create_dir(&state).unwrap();
        let name = format!("root::find_in_files::parity_tests::{scenario}");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &name,
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("GUILD_STATE_DIR", &state)
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .env("GUILD_SEARCH_PARITY_OWNED", &base)
            .output()
            .unwrap();
        // output() waits for child exit; the parent owns all fixture directories
        // until then, including queued store writes flushed by the child guard.
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() || !stdout.contains("1 passed; 0 failed") {
            failures.push(format!(
                "{name}: {stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            ));
            continue;
        }
        let saved: guild_session::Desk =
            serde_json::from_slice(&std::fs::read(state.join("workspace.json")).unwrap()).unwrap();
        assert!(saved.projects().count() > 0);
        assert!(
            saved
                .projects()
                .all(|project| { project.path.canonicalize().unwrap().starts_with(&base) })
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn isolated_base() -> std::path::PathBuf {
    let base = std::path::PathBuf::from(
        std::env::var_os("GUILD_SEARCH_PARITY_OWNED")
            .expect("run through isolated_project_search_parity"),
    );
    assert!(base.is_absolute());
    let base = base.canonicalize().unwrap();
    let state = base.join("state").canonicalize().unwrap();
    assert_eq!(crate::paths::state_dir().canonicalize().unwrap(), state);
    for path in [
        crate::paths::workspace_file(),
        crate::paths::settings_file(),
        crate::paths::keymap_file(),
    ] {
        assert_eq!(path.parent().unwrap().canonicalize().unwrap(), state);
    }
    assert_eq!(std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref(), Ok("1"));
    base
}

// The wrapper, rather than individual test locals, owns deletion after child exit.
struct OwnedDirectory(std::path::PathBuf);
impl OwnedDirectory {
    fn new() -> Self {
        let base = isolated_base();
        let path = tempfile::tempdir_in(&base).unwrap().keep();
        assert!(path.canonicalize().unwrap().starts_with(base));
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
struct FlushWrites;
impl Drop for FlushWrites {
    fn drop(&mut self) {
        guild_session::store::flush();
    }
}

fn fixture<'a>(
    cx: &'a mut TestAppContext,
    path: &Path,
    state: &Path,
) -> (Entity<Root>, &'a mut VisualTestContext) {
    let allowed = isolated_base();
    assert!(path.canonicalize().unwrap().starts_with(&allowed));
    assert!(state.canonicalize().unwrap().starts_with(&allowed));
    let mut made = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let inherited = cx.global::<crate::actions::Inherited>().0.clone();
        cx.bind_keys(crate::actions::ordered(
            vec![
                ("cmd-f".into(), crate::actions::keymap::Named::Find),
                ("cmd-r".into(), crate::actions::keymap::Named::Replace),
                (
                    "cmd-shift-f".into(),
                    crate::actions::keymap::Named::FindInFiles,
                ),
                (
                    "cmd-shift-r".into(),
                    crate::actions::keymap::Named::ReplaceInFiles,
                ),
            ],
            &inherited,
        ));
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), path);
        desk.look_at(Some(project), None);
        let root = cx.new(|cx| {
            // The waiting parent inspects these snapshots after case-local
            // scratch directories have been removed. Its child-owned directory
            // therefore owns the Root's runtime and persistence together.
            let mut root = Root::diagnostic(desk, crate::paths::state_dir(), cx);
            root.active_project = Some(project);
            root.settings.autosave = false;
            root
        });
        root.update(cx, |root, cx| {
            root.open_file(Path::new("a.txt"), window, cx)
        });
        made = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    visual.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    (made.unwrap(), visual)
}
fn wait_scan(root: &Entity<Root>, cx: &mut VisualTestContext) {
    for _ in 0..400 {
        let done = cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                root.find_in_files_poll(cx);
                root.find_in_files_popup()
                    .is_some_and(|popup| !popup.walk.running())
            })
        });
        if done {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
        cx.run_until_parked();
    }
    panic!("bounded project scan did not finish");
}
#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn project_replace_keeps_dirty_buffers_refuses_stale_disk_and_undoes_per_file(
    cx: &mut TestAppContext,
) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    for path in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(dir.path().join(path), "cat cat").unwrap();
    }
    let (root, cx) = fixture(cx, dir.path(), state.path());
    let file = cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let Some(Body::File { tab, .. }) = root.active_body() else {
                panic!("file")
            };
            let file = tab.clone();
            let editor = file.read(cx).editor().unwrap().clone();
            editor.update(cx, |editor, cx| editor.insert("prior ", window, cx));
            file
        })
    });
    cx.simulate_keystrokes("cmd-shift-r");
    cx.run_until_parked();
    cx.simulate_input("cat");
    wait_scan(&root, cx);
    cx.update(|_, cx| {
        let popup = root.read(cx).find_in_files_popup().unwrap();
        assert!(popup.replace_mode);
        assert_eq!(popup.walk.documents().len(), 3);
        assert_eq!(
            popup
                .walk
                .documents()
                .iter()
                .find(|d| d.path == Path::new("a.txt"))
                .unwrap()
                .text,
            "prior cat cat"
        );
    });
    std::fs::write(dir.path().join("c.txt"), "external").unwrap();
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.find_in_files_popup_mut().unwrap().replacement = "dog".into();
            root.find_in_files_replace(true, window, cx);
        })
    });
    for _ in 0..400 {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(2));
        let done = cx.update(|_, cx| {
            root.read(cx)
                .find_in_files_popup()
                .is_some_and(|p| !p.applying)
        });
        if done {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    cx.update(|_, cx| {
        let editor = file.read(cx).editor().unwrap().read(cx);
        assert_eq!(editor.text().to_string(), "prior dog dog");
        let root = root.read(cx);
        let popup = root.find_in_files_popup().unwrap();
        assert!(
            popup.replace_status.contains("4 replaced"),
            "{}",
            popup.replace_status
        );
        assert!(
            popup.replace_status.contains("1 refused"),
            "{}",
            popup.replace_status
        );
        let b = root
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|body| match body {
                Body::File {
                    path,
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } if path == Path::new("b.txt") => Some(tab),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            b.read(cx).editor().unwrap().read(cx).text().to_string(),
            "dog dog"
        );
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "cat cat",
        "autosave off: edits stay in the existing buffer"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "cat cat"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("c.txt")).unwrap(),
        "external"
    );
    cx.update(|window, cx| {
        file.read(cx)
            .editor()
            .unwrap()
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
        window.dispatch_action(Box::new(gpui_component::input::Undo), cx);
    });
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).text().to_string(),
            "prior cat cat"
        )
    });
}
#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn double_shift_reaches_everywhere_through_a_focused_file(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "source").unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    for modifiers in [
        gpui::Modifiers::shift(),
        gpui::Modifiers::none(),
        gpui::Modifiers::shift(),
        gpui::Modifiers::none(),
    ] {
        cx.simulate_modifiers_change(modifiers);
    }
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            root.read(cx).everywhere().is_some(),
            "actual Root modifier listener must open Search Everywhere over editor focus"
        )
    });
    cx.simulate_input("a.txt");
    cx.update(|_, cx| assert_eq!(root.read(cx).picker_query(), "a.txt"));
}
use gpui::Focusable;

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn project_query_replacement_and_multiline_keep_their_own_input(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "cat\ncat").unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    cx.simulate_keystrokes("cmd-shift-r");
    cx.run_until_parked();
    cx.simulate_input("cat");
    wait_scan(&root, cx);
    cx.simulate_keystrokes("tab");
    cx.simulate_input("dog");
    cx.update(|_, cx| {
        let popup = root.read(cx).find_in_files_popup().unwrap();
        assert_eq!(popup.query.text, "cat");
        assert_eq!(popup.replacement, "dog");
    });
    cx.simulate_keystrokes("shift-tab alt-w");
    cx.update(|_, cx| {
        assert!(
            root.read(cx)
                .find_in_files_popup()
                .unwrap()
                .query
                .whole_word
        )
    });
    let bounds = cx.debug_bounds("project-multiline").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("cat\ncat");
    wait_scan(&root, cx);
    cx.update(|_, cx| {
        let root = root.read(cx);
        let popup = root.find_in_files_popup().unwrap();
        assert_eq!(popup.query.text, "cat\ncat");
        assert_eq!(
            popup
                .walk
                .documents()
                .iter()
                .map(|document| document.match_count)
                .sum::<usize>(),
            1
        );
        let Some(Body::File { tab, .. }) = root.active_body() else {
            panic!("file")
        };
        assert_eq!(
            tab.read(cx).editor().unwrap().read(cx).text().to_string(),
            "cat\ncat"
        );
    });
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(root.read(cx).find_in_files_popup().is_none()));
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn each_file_is_revalidated_after_an_earlier_file_was_applied(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    for path in ["a.txt", "b.txt"] {
        std::fs::write(dir.path().join(path), "cat cat").unwrap();
    }
    let (root, cx) = fixture(cx, dir.path(), state.path());
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.open_file(Path::new("b.txt"), window, cx)
        })
    });
    cx.simulate_keystrokes("cmd-shift-r");
    cx.run_until_parked();
    cx.simulate_input("cat");
    wait_scan(&root, cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.find_in_files_popup_mut().unwrap().replacement = "dog".into();
            root.find_in_files_replace(true, window, cx);
        })
    });
    cx.run_until_parked();
    let (path, pending) = cx.update(|_, cx| {
        let root = root.read(cx);
        assert!(
            root.find_in_files_popup()
                .unwrap()
                .replace_status
                .starts_with("2 replaced"),
            "first file must already be applied before the external change"
        );
        root.tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|body| match body {
                Body::File {
                    path,
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } if tab.read(cx).editor().unwrap().read(cx).text() == "cat cat" => {
                    Some((path.clone(), tab.clone()))
                }
                _ => None,
            })
            .expect("second open buffer has not been applied yet")
    });
    std::fs::write(dir.path().join(&path), "late external change").unwrap();
    for _ in 0..20 {
        cx.executor().advance_clock(Duration::from_millis(2));
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        assert_eq!(
            pending
                .read(cx)
                .editor()
                .unwrap()
                .read(cx)
                .text()
                .to_string(),
            "cat cat",
            "late stale disk must refuse even an unchanged open buffer"
        )
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join(path)).unwrap(),
        "late external change"
    );
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn exclude_and_cancel_preserve_unprocessed_documents(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    for path in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(dir.path().join(path), "cat cat").unwrap();
    }
    let (root, cx) = fixture(cx, dir.path(), state.path());
    cx.simulate_keystrokes("cmd-shift-r");
    cx.run_until_parked();
    cx.simulate_input("cat");
    wait_scan(&root, cx);
    let excluded = cx.update(|_, cx| {
        let root = root.read(cx);
        let row = root.picker.as_ref().unwrap().picker.lit().unwrap();
        root.find_in_files_popup().unwrap().walk.hits()[row]
            .path
            .clone()
    });
    let exclude = cx.debug_bounds("project-exclude").unwrap();
    cx.simulate_click(exclude.center(), Default::default());
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.find_in_files_popup_mut().unwrap().replacement = "dog".into();
            root.find_in_files_replace(true, window, cx);
        })
    });
    cx.run_until_parked();
    let cancel = cx
        .debug_bounds("project-cancel-replace")
        .expect("Cancel remains available between files");
    cx.simulate_click(cancel.center(), Default::default());
    for _ in 0..10 {
        cx.executor().advance_clock(Duration::from_millis(2));
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let root = root.read(cx);
        let popup = root.find_in_files_popup().unwrap();
        assert!(!popup.applying);
        assert!(
            popup
                .excluded
                .contains(&dir.path().join(&excluded).canonicalize().unwrap())
        );
        assert!(
            popup
                .replace_status
                .contains("cancelled; 1 unprocessed files unchanged")
        );
        let changed = root
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .filter_map(|body| match body {
                Body::File {
                    path,
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } => Some((
                    path,
                    tab.read(cx).editor().unwrap().read(cx).text().to_string(),
                )),
                _ => None,
            })
            .filter(|(_, text)| text == "dog dog")
            .count();
        assert_eq!(
            changed, 1,
            "Cancel preserves only the already completed file"
        );
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&excluded)).unwrap(),
        "cat cat"
    );
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn watcher_refresh_waits_for_replacement_completion(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    for path in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(dir.path().join(path), "cat cat").unwrap();
    }
    let (root, cx) = fixture(cx, dir.path(), state.path());
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            root.settings.autosave = true;
            let look = root.file_look();
            let ui = root.ui();
            for body in root.tabs.iter().flat_map(crate::Tab::bodies) {
                if let Body::File { tab, .. } = body {
                    tab.update(cx, |file, cx| file.restyle(look.clone(), ui, cx));
                }
            }
        })
    });
    cx.simulate_keystrokes("cmd-shift-r");
    cx.simulate_input("cat");
    wait_scan(&root, cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.find_in_files_popup_mut().unwrap().replacement = "dog".into();
            root.find_in_files_replace(true, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            // Exercise the actual autosave path before delivering its watcher batch.
            let (applied_path, file) = root
                .tabs
                .iter()
                .flat_map(crate::Tab::bodies)
                .find_map(|body| {
                    if let Body::File {
                        path,
                        tab,
                        origin: crate::Origin::Project,
                        ..
                    } = body
                        && tab.read(cx).editor().unwrap().read(cx).text() == "dog dog"
                    {
                        Some((path.clone(), tab.clone()))
                    } else {
                        None
                    }
                })
                .expect("one file applied before watcher refresh");
            // Returning focus to the popup may already save on Blur. If it has
            // not, Pause uses the same real save path; disk bytes prove either.
            let verdict = file.update(cx, |file, cx| {
                file.autosave(crate::autosave::Trigger::Pause, cx)
            });
            assert!(matches!(
                verdict,
                crate::autosave::Verdict::Write
                    | crate::autosave::Verdict::Refuse(crate::autosave::Refusal::Clean)
            ));
            assert_eq!(
                std::fs::read_to_string(dir.path().join(applied_path)).unwrap(),
                "dog dog"
            );
            let project = root.active_project.unwrap();
            let mut batch = crate::watch::Batch::new(&ignore::gitignore::Gitignore::empty());
            batch.push(Path::new("a.txt"));
            root.find_in_files_heard(project, &batch);
            let popup = root.find_in_files_popup_mut().unwrap();
            assert!(popup.applying && popup.apply_task.is_some());
            let generation = popup.walk.generation();
            let expired = std::time::Instant::now() - Duration::from_secs(3);
            popup.stale = Some((expired, expired));
            root.find_in_files_poll(cx);
            let popup = root.find_in_files_popup().unwrap();
            assert_eq!(popup.walk.generation(), generation);
            assert!(popup.applying && popup.apply_task.is_some());
            assert!(popup.stale.is_some());
        })
    });
    for _ in 0..100 {
        cx.executor().advance_clock(Duration::from_millis(2));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(2));
    }
    cx.update(|_, cx| {
        let root = root.read(cx);
        let popup = root.find_in_files_popup().unwrap();
        assert!(!popup.applying);
        assert!(
            popup
                .replace_status
                .contains("6 replaced in editor buffers"),
            "{}",
            popup.replace_status
        );
    });
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn reinvocation_selects_the_actual_query_component(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "cat dog").unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    cx.simulate_keystrokes("cmd-shift-r");
    cx.simulate_input("cat");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("replacement");
    for stroke in ["cmd-shift-r", "cmd-shift-f"] {
        cx.simulate_keystrokes(stroke);
        cx.run_until_parked();
        cx.simulate_input("dog");
        cx.update(|_, cx| {
            let root = root.read(cx);
            let popup = root.find_in_files_popup().unwrap();
            assert_eq!(popup.query.text, "dog");
            assert_eq!(popup.replacement, "replacement");
            assert!(!popup.replacement_focus);
        });
        if stroke == "cmd-shift-r" {
            cx.simulate_keystrokes("tab");
        }
    }
    let bounds = cx.debug_bounds("project-multiline").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("cat\ndog");
    cx.simulate_keystrokes("cmd-shift-r");
    cx.run_until_parked();
    cx.simulate_input("dog\ncat");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).find_in_files_popup().unwrap().query.text,
            "dog\ncat"
        )
    });
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn late_divergent_alias_refuses_replacement(cx: &mut TestAppContext) {
    late_alias_scenario(cx, false);
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn late_alias_in_another_open_project_refuses_replacement(cx: &mut TestAppContext) {
    late_alias_scenario(cx, true);
}

fn late_alias_scenario(cx: &mut TestAppContext, other_project: bool) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "cat cat").unwrap();
    std::os::unix::fs::symlink("a.txt", dir.path().join("link.txt")).unwrap();
    let alias_dir = OwnedDirectory::new();
    std::os::unix::fs::symlink(dir.path().join("a.txt"), alias_dir.path().join("link.txt"))
        .unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    cx.simulate_keystrokes("cmd-shift-r");
    cx.simulate_input("cat");
    wait_scan(&root, cx);
    let original = cx.update(|_, cx| root.read(cx).active_project);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            if other_project {
                let project = root
                    .desk
                    .open_in(root.desk.active_workspace(), alias_dir.path());
                root.active_project = Some(project);
            }
            root.open_file(Path::new("link.txt"), window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let Some(Body::File { tab, .. }) = root.active_body() else {
                panic!("alias")
            };
            tab.read(cx)
                .editor()
                .unwrap()
                .clone()
                .update(cx, |editor, cx| editor.insert("unsaved ", window, cx));
            root.active_project = original;
            root.find_in_files_popup_mut().unwrap().replacement = "dog".into();
            root.find_in_files_replace(true, window, cx);
        })
    });
    for _ in 0..100 {
        cx.executor().advance_clock(Duration::from_millis(2));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(2));
    }
    cx.update(|_, cx| {
        let root = root.read(cx);
        assert!(
            root.find_in_files_popup()
                .unwrap()
                .replace_status
                .contains("1 refused")
        );
        for body in root.tabs.iter().flat_map(crate::Tab::bodies) {
            if let Body::File { tab, .. } = body {
                assert!(
                    !tab.read(cx)
                        .editor()
                        .unwrap()
                        .read(cx)
                        .text()
                        .to_string()
                        .contains("dog")
                );
            }
        }
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "cat cat"
    );
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn excluded_identity_survives_new_display_alias(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "cat cat").unwrap();
    std::os::unix::fs::symlink("a.txt", dir.path().join("link.txt")).unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    // Search without an open owner, then make the alias the only owner.
    cx.update(|_, cx| {
        root.update(cx, |root, _| {
            root.tabs.clear();
            root.active_tab = None;
        })
    });
    cx.simulate_keystrokes("cmd-shift-r");
    cx.simulate_input("cat");
    wait_scan(&root, cx);
    let exclude = cx.debug_bounds("project-exclude").unwrap();
    cx.simulate_click(exclude.center(), Default::default());
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.open_file(Path::new("link.txt"), window, cx)
        })
    });
    cx.simulate_keystrokes("cmd-shift-r");
    cx.simulate_input("cat cat");
    wait_scan(&root, cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            assert_eq!(
                root.find_in_files_popup().unwrap().walk.documents()[0].path,
                Path::new("link.txt")
            );
            root.find_in_files_popup_mut().unwrap().replacement = "dog".into();
            root.find_in_files_replace(true, window, cx);
        })
    });
    for _ in 0..100 {
        cx.executor().advance_clock(Duration::from_millis(2));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(2));
    }
    cx.update(|_, cx| {
        let root = root.read(cx);
        let Some(Body::File { tab, .. }) = root.active_body() else {
            panic!("alias")
        };
        assert_eq!(
            tab.read(cx).editor().unwrap().read(cx).text().to_string(),
            "cat cat"
        );
    });
    // Include on the new alias must remove the same canonical exclusion.
    let include = cx.debug_bounds("project-exclude").unwrap();
    cx.simulate_click(include.center(), Default::default());
    cx.update(|_, cx| {
        assert!(
            root.read(cx)
                .find_in_files_popup()
                .unwrap()
                .excluded
                .is_empty()
        )
    });
}

pub(super) struct ValidationGate {
    pub entered: tokio::sync::oneshot::Sender<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub release: tokio::sync::oneshot::Receiver<()>,
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn alias_roster_change_during_validation_refuses_the_edit(cx: &mut TestAppContext) {
    pending_alias_validation(cx, false);
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn cancel_during_background_validation_leaves_buffers_unchanged(cx: &mut TestAppContext) {
    pending_alias_validation(cx, true);
}

fn pending_alias_validation(cx: &mut TestAppContext, cancel: bool) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "cat cat").unwrap();
    std::os::unix::fs::symlink("a.txt", dir.path().join("link.txt")).unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    cx.simulate_keystrokes("cmd-shift-r");
    cx.simulate_input("cat");
    wait_scan(&root, cx);
    let (entered_send, mut entered) = tokio::sync::oneshot::channel();
    let (release, release_recv) = tokio::sync::oneshot::channel();
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let popup = root.find_in_files_popup_mut().unwrap();
            popup.replacement = "dog".into();
            popup.validation_gate = Some(ValidationGate {
                entered: entered_send,
                release: release_recv,
            });
            root.find_in_files_replace(true, window, cx);
        })
    });
    cx.run_until_parked();
    let stopped = entered
        .try_recv()
        .expect("validation paused at the explicit background gate");
    assert!(!stopped.load(std::sync::atomic::Ordering::Relaxed));
    if cancel {
        let bounds = cx.debug_bounds("project-cancel-replace").unwrap();
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
        assert!(
            stopped.load(std::sync::atomic::Ordering::Relaxed),
            "cancellation must stop pending background alias work"
        );
    } else {
        // Even a clean alias changes the validated roster. This must refuse
        // rather than silently ignoring a buffer opened during path resolution.
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                root.open_file(Path::new("link.txt"), window, cx)
            })
        });
    }
    let _ = release.send(());
    cx.run_until_parked();
    for _ in 0..20 {
        cx.executor().advance_clock(Duration::from_millis(2));
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let root = root.read(cx);
        let popup = root.find_in_files_popup().unwrap();
        assert!(!popup.applying);
        assert!(
            popup.replace_status.contains(if cancel {
                "cancelled"
            } else {
                "Open files changed"
            }),
            "{}",
            popup.replace_status
        );
        for body in root.tabs.iter().flat_map(crate::Tab::bodies) {
            if let Body::File { tab, .. } = body {
                assert_eq!(
                    tab.read(cx).editor().unwrap().read(cx).text().to_string(),
                    "cat cat"
                );
            }
        }
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "cat cat"
    );
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn single_project_replace_only_checks_selected_output(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "cat".repeat(30_000)).unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    cx.simulate_keystrokes("cmd-shift-r");
    cx.simulate_input("cat");
    wait_scan(&root, cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.find_in_files_popup_mut().unwrap().replacement = "a".repeat(1024);
            root.find_in_files_replace(false, window, cx);
        })
    });
    for _ in 0..40 {
        cx.executor().advance_clock(Duration::from_millis(2));
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let root = root.read(cx);
        assert!(
            root.find_in_files_popup()
                .unwrap()
                .replace_status
                .contains("1 replaced in editor buffers"),
            "{}",
            root.find_in_files_popup().unwrap().replace_status
        );
        let Some(Body::File { tab, .. }) = root.active_body() else {
            panic!("file")
        };
        assert_eq!(
            tab.read(cx).editor().unwrap().read(cx).text().len(),
            90_000 - 3 + 1024
        );
    });
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn multiline_tab_cycles_focus_actual_query_mask_and_replacement(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "cat\ncat").unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    cx.simulate_keystrokes("cmd-shift-f");
    let bounds = cx.debug_bounds("project-multiline").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.simulate_input("cat\ncat");
    cx.simulate_keystrokes("shift-tab cmd-a");
    cx.simulate_input("*.txt");
    cx.update(|_, cx| {
        let root = root.read(cx);
        let p = root.find_in_files_popup().unwrap();
        assert_eq!(p.query.text, "cat\ncat");
        assert_eq!(p.query.mask, "*.txt");
    });
    cx.simulate_keystrokes("tab cmd-a");
    cx.simulate_input("dog\ncat");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).find_in_files_popup().unwrap().query.text,
            "dog\ncat"
        )
    });
    cx.simulate_keystrokes("cmd-shift-r tab");
    cx.simulate_input("replacement");
    cx.simulate_keystrokes("tab cmd-a");
    cx.simulate_input("*.rs");
    cx.simulate_keystrokes("tab cmd-a");
    cx.simulate_input("cat\ndog");
    cx.update(|_, cx| {
        let root = root.read(cx);
        let p = root.find_in_files_popup().unwrap();
        assert_eq!(p.query.text, "cat\ndog");
        assert_eq!(p.replacement, "replacement");
        assert_eq!(p.query.mask, "*.rs");
    });
    cx.simulate_keystrokes("shift-tab cmd-a");
    cx.simulate_input("*.txt");
    cx.simulate_keystrokes("shift-tab cmd-a");
    cx.simulate_input("new replacement");
    cx.simulate_keystrokes("shift-tab cmd-a");
    cx.simulate_input("cat\ncat");
    cx.update(|_, cx| {
        let root = root.read(cx);
        let p = root.find_in_files_popup().unwrap();
        assert_eq!(p.query.text, "cat\ncat");
        assert_eq!(p.replacement, "new replacement");
        assert_eq!(p.query.mask, "*.txt");
    });
}

#[test]
fn single_line_plan_stops_at_the_first_match_and_cancels_deterministically() {
    let query = crate::search::Query {
        text: "cat".into(),
        ..Default::default()
    };
    let small = "x".repeat(4096) + &"cat".repeat(100);
    let mut work = 0;
    let plan = super::prepare_document_edits(&small, &query, "dog", false, Some(1), || {
        work += 1;
        false
    })
    .unwrap()
    .unwrap();
    assert_eq!(plan.len(), 1);
    assert!(
        work < 16,
        "single replacement must not collect every occurrence: {work}"
    );
    // The small work assertion above fails before this dense stress case on the old algorithm.
    let dense = "я".repeat(5 * 1024 * 1024) + &"cat".repeat(50_000);
    let plan = super::prepare_document_edits(&dense, &query, "dog", false, Some(1), || false)
        .unwrap()
        .unwrap();
    assert_eq!(plan[0].range, 10 * 1024 * 1024..10 * 1024 * 1024 + 3);
    let mut checks = 0;
    assert!(
        super::prepare_document_edits(&dense, &query, "dog", false, Some(1), || {
            checks += 1;
            checks == 5
        })
        .unwrap()
        .is_none()
    );
    assert_eq!(checks, 5);
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn floating_picker_tracks_drag_over_inputs_and_final_release(cx: &mut TestAppContext) {
    use gpui::{MouseButton, point, px};
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "cat").unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    for action in [
        Box::new(crate::actions::RecentFiles) as Box<dyn gpui::Action>,
        Box::new(crate::actions::FindInFiles),
    ] {
        cx.update(|window, cx| window.dispatch_action(action, cx));
        cx.run_until_parked();
        // Start on the header's free padding, then move under the text input.
        let head = cx.debug_bounds("picker-head").unwrap();
        let start = point(head.right() - px(20.), head.top() + px(3.));
        cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
        cx.update(|_, cx| assert!(root.read(cx).picker_held()));
        let before = cx.debug_bounds("picker-head").unwrap();
        let to = point(head.left() + px(80.), head.top() + px(20.));
        cx.simulate_mouse_move(to, MouseButton::Left, Default::default());
        cx.run_until_parked();
        let after = cx.debug_bounds("picker-head").unwrap();
        assert_ne!(before.origin, after.origin, "drag must cross a child input");
        let release = to + point(px(17.), px(13.));
        cx.simulate_mouse_up(release, MouseButton::Left, Default::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert!(
                !root.read(cx).picker_held(),
                "release over field must finish"
            )
        });
        let final_head = cx.debug_bounds("picker-head").unwrap();
        assert_eq!(
            final_head.origin,
            after.origin + point(px(17.), px(13.)),
            "release position must be applied"
        );
        // A release outside the window may be absent; the next move over an
        // input still has to end the grab, without moving the card again.
        let start = point(final_head.right() - px(20.), final_head.top() + px(3.));
        cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
        cx.update(|_, cx| assert!(root.read(cx).picker_held()));
        let unpressed = final_head.origin + point(px(50.), px(20.));
        cx.simulate_mouse_move(unpressed, None, Default::default());
        cx.run_until_parked();
        cx.update(|_, cx| assert!(!root.read(cx).picker_held()));
        assert_eq!(
            cx.debug_bounds("picker-head").unwrap().origin,
            final_head.origin
        );
        cx.simulate_mouse_up(unpressed, MouseButton::Left, Default::default());
        cx.simulate_keystrokes("escape");
    }
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn recent_files_is_a_project_mru_selector_without_text_input(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(dir.path().join(name), name).unwrap();
    }
    let (root, cx) = fixture(cx, dir.path(), state.path());
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            for name in ["b.txt", "c.txt", "a.txt"] {
                root.open_file(Path::new(name), window, cx);
            }
            let project = root.active_project.unwrap();
            assert_eq!(
                root.recent_files_of(project),
                ["a.txt", "c.txt", "b.txt"].map(std::path::PathBuf::from)
            );
            window.dispatch_action(Box::new(crate::actions::RecentFiles), cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            !crate::text_field::focused(window, cx),
            "Recent Files is a selector"
        );
        let root = root.read(cx);
        assert_eq!(root.picker_lit_row(), Some(1));
        assert_eq!(root.picker_query(), "");
    });
    cx.simulate_keystrokes("z");
    cx.simulate_input("never a hidden query");
    cx.update(|_, cx| {
        let root = root.read(cx);
        assert_eq!(root.picker_query(), "");
        let Some(Body::File { tab, .. }) = root.active_body() else {
            panic!("file")
        };
        assert_eq!(
            tab.read(cx).editor().unwrap().read(cx).text().to_string(),
            "a.txt"
        );
    });
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let root = root.read(cx);
        assert!(root.picker.is_none());
        assert!(matches!(root.active_body(), Some(Body::File { path, origin: crate::Origin::Project, .. }) if path == Path::new("b.txt")));
    });
    cx.update(|window, cx| window.dispatch_action(Box::new(crate::actions::RecentFiles), cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx)
                .recent_files_rows()
                .into_iter()
                .map(|(_, p)| p)
                .collect::<Vec<_>>(),
            ["b.txt", "a.txt", "c.txt"].map(std::path::PathBuf::from)
        );
    });

    let edited = cx.debug_bounds("recent-files-edited").unwrap();
    cx.simulate_click(edited.center(), Default::default());
    cx.update(|_, cx| assert!(root.read(cx).recent_files_rows().is_empty()));
    cx.simulate_click(edited.center(), Default::default());
    cx.update(|_, cx| assert_eq!(root.read(cx).recent_files_rows().len(), 3));
    // Backspace on a dirty row must not forget it when its sheet is cancelled.
    cx.update(|window, cx| {
        let file = root
            .read(cx)
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|body| match body {
                Body::File {
                    path,
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } if path == Path::new("a.txt") => Some(tab.clone()),
                _ => None,
            })
            .unwrap();
        let editor = file.read(cx).editor().unwrap().clone();
        editor.update(cx, |editor, cx| {
            editor.replace_all("dirty a.txt", window, cx)
        });
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            root.poll_picker(cx);
            assert!(
                root.recent_files_rows()
                    .iter()
                    .any(|(_, p)| p == Path::new("a.txt"))
            );
        })
    });
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            root.poll_picker(cx);
            assert!(
                !root
                    .recent_files_rows()
                    .iter()
                    .any(|(_, p)| p == Path::new("a.txt"))
            );
        })
    });
    cx.simulate_keystrokes("escape");
    let other = OwnedDirectory::new();
    std::fs::write(other.path().join("a.txt"), "other project").unwrap();
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let original = root.active_project.unwrap();
            let project = root.desk.open_in(root.active_workspace, other.path());
            root.active_project = Some(project);
            root.open_file(Path::new("a.txt"), window, cx);
            assert_eq!(
                root.recent_files_of(project),
                [std::path::PathBuf::from("a.txt")]
            );
            assert_eq!(
                root.recent_files_of(original),
                ["b.txt", "c.txt"].map(std::path::PathBuf::from)
            );
            assert_eq!(
                guild_session::project_state::load(dir.path()).recent_files,
                ["b.txt", "c.txt"].map(std::path::PathBuf::from)
            );
        })
    });
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn cancelled_recent_close_cannot_forget_a_later_unrelated_close(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    for name in ["a.txt", "b.txt"] {
        std::fs::write(dir.path().join(name), name).unwrap();
    }
    let (root, cx) = fixture(cx, dir.path(), state.path());
    let mut target = None;
    cx.update(|window, cx| root.update(cx, |root, cx| {
        root.open_file(Path::new("b.txt"), window, cx);
        root.open_file(Path::new("a.txt"), window, cx);
        target = root.tabs.iter().find(|tab| tab.bodies().any(|body| matches!(body, Body::File { path, origin: crate::Origin::Project, .. } if path == Path::new("a.txt")))).map(|tab| tab.id);
        let Some(Body::File { tab, .. }) = root.active_body() else { panic!("file") };
        let editor = tab.read(cx).editor().unwrap().clone();
        editor.update(cx, |editor, cx| editor.replace_all("dirty a", window, cx));
        window.dispatch_action(Box::new(crate::actions::RecentFiles), cx);
    }));
    cx.run_until_parked();
    cx.simulate_keystrokes("up backspace");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            assert!(!root.asking);
            // No poll/watch tick between cancellation and the unrelated gesture.
            root.close_tab_asking(target.unwrap(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            assert!(
                root.picker.is_some(),
                "keep the same selector to expose a delayed pending intent"
            );
            root.poll_picker(cx);
            assert!(
                root.recent_files_of(root.active_project.unwrap())
                    .contains(&"a.txt".into()),
                "cancelled Backspace must not forget a later ordinary close"
            );
        })
    });
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn titlebar_shortcuts_toggle_and_restore_focus_with_overrides(cx: &mut TestAppContext) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "cat").unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    let named = crate::actions::keymap::parse(
        r#"{"ctrl-cmd-j":"ToggleTerminalJobs","ctrl-cmd-u":"ToggleUsage"}"#,
    )
    .expect("named titlebar toggles must be rebindable");
    for custom in [false, true] {
        let jobs = if custom {
            "ctrl-cmd-alt-j"
        } else {
            "ctrl-cmd-j"
        };
        let usage = if custom {
            "ctrl-cmd-alt-u"
        } else {
            "ctrl-cmd-u"
        };
        if custom {
            crate::actions::keymap::save(
                &named
                    .iter()
                    .map(|(stroke, action)| (*action, stroke.replace("ctrl-cmd-", "ctrl-cmd-alt-")))
                    .collect(),
            )
            .unwrap();
        }
        cx.update(|_, cx| crate::actions::rebind(cx));
        cx.run_until_parked();
        let previous = cx.update(|window, cx| window.focused(cx).unwrap());
        cx.simulate_keystrokes(jobs);
        cx.run_until_parked();
        cx.update(|_, cx| assert!(root.read(cx).terminal_list.is_some()));
        cx.simulate_keystrokes(jobs);
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert!(root.read(cx).terminal_list.is_none());
            assert!(previous.is_focused(window));
        });
        cx.simulate_keystrokes(usage);
        cx.run_until_parked();
        cx.update(|_, cx| assert!(root.read(cx).showing_usage));
        cx.simulate_keystrokes(usage);
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert!(!root.read(cx).showing_usage);
            assert!(previous.is_focused(window));
        });
        if custom {
            cx.simulate_keystrokes("ctrl-cmd-j ctrl-cmd-u");
            cx.update(|_, cx| {
                assert!(root.read(cx).terminal_list.is_none());
                assert!(!root.read(cx).showing_usage);
            });
        }
    }
    crate::actions::keymap::save(&Default::default()).unwrap();
}

// Exercise all three consumers of the common overlay collector through their
// real restart paths, while file completion cannot yet publish on the UI thread.
#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn pending_file_searches_keep_disk_matches_and_empty_buffers_override_them(
    cx: &mut TestAppContext,
) {
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "needle disk\n").unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    for restored in [false, true] {
        for surface in 0..3 {
            let file = cx.update(|window, cx| {
                root.update(cx, |root, cx| {
                    let project = root.active_project.unwrap();
                    let ids: Vec<_> = root
                        .tabs
                        .iter()
                        .filter(|t| t.bodies().any(|b| matches!(b, Body::File { .. })))
                        .map(|t| t.id)
                        .collect();
                    for id in ids {
                        root.close_tab(id, cx);
                    }
                    if restored {
                        let id = root.restore_file_tab(project, dir.path(), "a.txt".into(), cx);
                        root.active_tab = Some(id);
                    } else {
                        root.open_file(Path::new("a.txt"), window, cx);
                    }
                    let Some(Body::File { tab, .. }) = root.active_body() else {
                        panic!("file")
                    };
                    let file = tab.clone();
                    assert!(file.read(cx).source_loading());
                    assert!(file.read(cx).editor().is_none());
                    start_overlay_search(root, surface, window, cx);
                    file
                })
            });
            wait_overlay_search(&root, surface, 1, cx);
            // Completion must not erase the already captured disk result.
            cx.update(|window, cx| file.update(cx, |file, cx| file.realise(window, cx)));
            cx.run_until_parked();
            cx.update(|_, cx| assert!(file.read(cx).editor().is_some()));
            wait_overlay_search(&root, surface, 1, cx);
            cx.update(|window, cx| {
                root.update(cx, |root, cx| {
                    start_overlay_search(root, surface, window, cx)
                })
            });
            wait_overlay_search(&root, surface, 1, cx);
            cx.update(|window, cx| {
                let editor = file.read(cx).editor().unwrap().clone();
                editor.update(cx, |editor, cx| editor.replace_all("", window, cx));
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                root.update(cx, |root, cx| {
                    assert!(file.read(cx).is_dirty());
                    start_overlay_search(root, surface, window, cx);
                })
            });
            wait_overlay_search(&root, surface, 0, cx);
            assert_eq!(
                std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
                "needle disk\n"
            );
            if restored && surface == 2 {
                cx.update(|_, cx| {
                    file.update(cx, |file, cx| file.save(cx).unwrap());
                    assert!(!file.read(cx).is_dirty());
                    let root = root.read(cx);
                    let buffers = root.project_search_buffers(root.active_project.unwrap(), cx);
                    assert_eq!(buffers[Path::new("a.txt")].text, "");
                });
            }
        }
    }
}

fn start_overlay_search(
    root: &mut Root,
    surface: usize,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Root>,
) {
    if root.picker.is_some() {
        root.close_picker(crate::root::picker::Focus::Back, window, cx);
    }
    match surface {
        0 => root.search_in_window(
            root.active_project.unwrap(),
            crate::search::Query {
                text: "needle".into(),
                ..Default::default()
            },
            window,
            cx,
        ),
        1 => {
            root.open_find_in_files(window, cx);
            let popup = root.find_in_files_popup_mut().unwrap();
            popup.query = crate::search::Query {
                text: "needle".into(),
                ..Default::default()
            };
            popup.replace_mode = true;
            root.find_in_files_restart(cx);
        }
        2 => {
            root.everywhere_pressed(crate::everywhere::Among::Text, window, cx);
            root.picker
                .as_mut()
                .unwrap()
                .picker
                .prefill("needle".into());
            root.everywhere_edited(window, cx);
        }
        _ => unreachable!(),
    }
}

fn wait_overlay_search(
    root: &Entity<Root>,
    surface: usize,
    expected: usize,
    cx: &mut VisualTestContext,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        let done = cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                root.poll_search(cx);
                root.poll_picker(cx);
                let walk = match surface {
                    0 => &root.searches[&root.active_project.unwrap()].walk,
                    1 => &root.find_in_files_popup().unwrap().walk,
                    2 => &root.everywhere().unwrap().text,
                    _ => unreachable!(),
                };
                if walk.running() {
                    return false;
                }
                assert_eq!(walk.hits().len(), expected, "search surface {surface}");
                true
            })
        });
        if done {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "search worker deadline"
        );
        std::thread::yield_now();
    }
}

#[gpui::test]
#[ignore = "run through isolated_project_search_parity"]
fn existing_working_diff_follows_late_source_editor_through_save_and_close(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking(); // The real diff worker wakes its UI subscriber.
    let _flush = FlushWrites;
    let dir = OwnedDirectory::new();
    let state = OwnedDirectory::new();
    std::fs::write(dir.path().join("a.txt"), "base\n").unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "a.txt"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-qm",
            "Fixture",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::write(dir.path().join("a.txt"), "disk\n").unwrap();
    let (root, cx) = fixture(cx, dir.path(), state.path());
    let other_dir = OwnedDirectory::new();
    std::fs::write(other_dir.path().join("a.txt"), "other project\n").unwrap();
    let (project, other, other_diff) = cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let project = root.active_project.unwrap();
            let other = root
                .desk
                .open_in(root.desk.active_workspace(), other_dir.path());
            let previous = root.active_tab;
            root.active_project = Some(other);
            root.open_diff(Some("a.txt".into()), guild_session::Base::Head, window, cx);
            let Some(Body::Diff { tab, .. }) = root.active_body() else {
                panic!("other diff")
            };
            let diff = tab.clone();
            root.active_project = Some(project);
            root.active_tab = previous;
            (project, other, diff)
        })
    });
    let diff = cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.close_tab(root.active_tab.unwrap(), cx);
            root.open_diff(Some("a.txt".into()), guild_session::Base::Head, window, cx);
            let Some(Body::Diff { tab, .. }) = root.active_body() else {
                panic!("diff")
            };
            tab.clone()
        })
    });
    wait_working_diff(&diff, "disk\n", cx);
    // The actual diff-to-source event takes Root's navigation/open-file route.
    cx.update(|_, cx| {
        diff.update(cx, |_, cx| {
            cx.emit(crate::diff_tab::DiffEvent::OpenAt {
                path: "a.txt".into(),
                line: 1,
            })
        })
    });
    // Completion belongs to the source's project even after switching away.
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            let Some(Body::File { tab, .. }) = root.active_body() else {
                panic!("source")
            };
            assert!(tab.read(cx).editor().is_none());
            root.active_project = Some(other);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| root.update(cx, |root, _| root.active_project = Some(project)));
    cx.update(|_, cx| assert!(!other_diff.read(cx).follows(Path::new("a.txt"))));
    let (id, file, editor) = cx.update(|_, cx| {
        let root = root.read(cx);
        let Some(Body::File { tab, .. }) = root.active_body() else {
            panic!("source")
        };
        (
            root.active_tab.unwrap(),
            tab.clone(),
            tab.read(cx).editor().unwrap().clone(),
        )
    });
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| editor.replace_all("unsaved\n", window, cx))
    });
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "disk\n"
    );
    wait_working_diff(&diff, "unsaved\n", cx);
    // Repeated lifecycle notifications must not replace a stable diff snapshot.
    let snapshot = cx.update(|_, cx| diff.read(cx).documents[Path::new("a.txt")].clone());
    cx.update(|_, cx| {
        file.update(cx, |_, cx| {
            cx.emit(crate::file_tab::BufferChanged);
            cx.emit(crate::file_tab::BufferChanged);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(std::sync::Arc::ptr_eq(
            &snapshot,
            &diff.read(cx).documents[Path::new("a.txt")]
        ))
    });
    cx.update(|_, cx| root.update(cx, |root, cx| root.save_file(id, cx).unwrap()));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "unsaved\n"
    );
    cx.update(|_, cx| root.update(cx, |root, cx| root.close_tab(id, cx)));
    cx.update(|_, cx| assert!(!diff.read(cx).follows(Path::new("a.txt"))));
    wait_working_diff(&diff, "unsaved\n", cx);
    // A closed editor retained by this test can no longer steer the diff.
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.replace_all("detached\n", window, cx)
        })
    });
    cx.run_until_parked();
    wait_working_diff(&diff, "unsaved\n", cx);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.open_file(Path::new("a.txt"), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let Some(Body::File { tab, .. }) = root.read(cx).active_body() else {
            panic!("source")
        };
        let editor = tab.read(cx).editor().unwrap().clone();
        editor.update(cx, |editor, cx| {
            editor.replace_all("reopened\n", window, cx)
        });
    });
    wait_working_diff(&diff, "reopened\n", cx);
}

fn wait_working_diff(
    diff: &Entity<crate::diff_tab::DiffTab>,
    expected: &str,
    cx: &mut VisualTestContext,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        let ready = cx.update(|_, cx| {
            diff.update(cx, |diff, cx| {
                diff.poll(cx);
                diff.documents
                    .get(Path::new("a.txt"))
                    .is_some_and(|d| d.sources[1].text.as_ref() == expected)
            })
        });
        if ready {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "diff never followed {expected:?}"
        );
        std::thread::yield_now();
    }
}
