//! GLD-057: an Explorer row that is, or holds, a root Guild has open is not
//! renamed, moved to the Trash, cut or moved — by a key or by a drop — and
//! the person is told why.
//!
//! A group folder's Explorer shows its open members as ordinary rows, and a
//! checkout can hold a worktree opened on its own; before the guard either
//! could be moved out from under the tabs and sessions Guild holds in it.

use super::explorer_fixture;
use crate::file_operations::Operation;
use crate::root::tree_drag::FileDrag;
use gpui::{Entity, TestAppContext, VisualTestContext};

fn click(cx: &mut VisualTestContext, path: &str, command: bool) {
    let selector: &'static str = Box::leak(format!("explorer-row-{path}").into_boxed_str());
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no row {path}"));
    cx.simulate_click(
        bounds.center(),
        gpui::Modifiers {
            platform: command,
            ..Default::default()
        },
    );
}

fn notices(root: &Entity<crate::Root>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        root.read(cx)
            .notices
            .visible()
            .map(|(_, notice)| notice.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn dismiss_notices(root: &Entity<crate::Root>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            loop {
                let Some(index) = r.notices.visible().map(|(index, _)| index).next() else {
                    break;
                };
                r.notices.dismiss(index);
            }
        })
    });
}

fn nothing_cut(root: &Entity<crate::Root>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| root.update(cx, |r, cx| r.cut_sources(cx).is_empty()))
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn an_open_root_in_the_explorer_is_not_renamed_trashed_cut_or_moved(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for folder in ["member/src", "outer/inner", "plain"] {
        std::fs::create_dir_all(dir.path().join(folder)).unwrap();
    }
    std::fs::write(dir.path().join("member/src/lib.rs"), "member").unwrap();
    // A second entry in each, so neither folds into a one-row chain
    // (`member/src`) and the rows are the roots themselves.
    std::fs::write(dir.path().join("member/README.md"), "member").unwrap();
    std::fs::write(dir.path().join("outer/notes.md"), "outer").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let (project, member, inner) = cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let workspace = r.active_workspace;
            let member = r.desk.open_in(workspace, &dir.path().join("member"));
            let inner = r.desk.open_in(workspace, &dir.path().join("outer/inner"));
            cx.notify();
            (r.active_project.unwrap(), member, inner)
        })
    });
    assert_ne!(project, member);
    assert_ne!(project, inner);
    cx.run_until_parked();
    let unmoved = |cx: &mut VisualTestContext| {
        cx.run_until_parked();
        assert!(dir.path().join("member/src/lib.rs").exists());
        assert!(dir.path().join("outer/inner").is_dir());
        assert!(!dir.path().join("plain/member").exists());
        assert!(!dir.path().join("plain/outer").exists());
        cx.update(|_, cx| assert_eq!(root.read(cx).active_project, Some(project)));
    };

    // Rename by its key: no field, and the reason in a notice.
    click(cx, "member", false);
    cx.simulate_keystrokes("shift-f6");
    cx.update(|window, cx| assert!(!crate::text_field::focused(window, cx)));
    let said = notices(&root, cx);
    assert!(
        said.contains("“member” is open in Guild as a project"),
        "{said}"
    );
    dismiss_notices(&root, cx);

    // Move to Trash by its key: refused before the question is asked.
    click(cx, "member", false);
    cx.simulate_keystrokes("cmd-backspace");
    assert!(!cx.has_pending_prompt(), "Trash asked about an open root");
    assert!(notices(&root, cx).contains("“member”"));
    dismiss_notices(&root, cx);

    // Cut, then Paste somewhere else: nothing is armed, nothing moves.
    click(cx, "member", false);
    cx.simulate_keystrokes("cmd-x");
    assert!(nothing_cut(&root, cx));
    click(cx, "plain", false);
    cx.simulate_keystrokes("cmd-v");
    unmoved(cx);
    dismiss_notices(&root, cx);

    // Move… by its key: the destination field never opens.
    click(cx, "member", false);
    cx.simulate_keystrokes("f6");
    cx.update(|_, cx| assert!(root.read(cx).file_operations.form.is_none()));
    unmoved(cx);
    dismiss_notices(&root, cx);

    // A drag does not start, and a drop's transfer is refused all the same.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let drag = FileDrag::new(project, "member".into());
            assert!(r.begin_tree_drag(&drag, window, cx).is_empty());
            r.start_transfer(
                project,
                vec!["member".into()],
                "plain".into(),
                Operation::Move,
                window,
                cx,
            );
        })
    });
    unmoved(cx);
    dismiss_notices(&root, cx);

    // A folder that holds an open root is refused the same way, and the
    // notice names the root inside it.
    click(cx, "outer", false);
    cx.simulate_keystrokes("shift-f6");
    cx.update(|window, cx| assert!(!crate::text_field::focused(window, cx)));
    let said = notices(&root, cx);
    assert!(said.contains("“outer/inner”"), "{said}");
    dismiss_notices(&root, cx);
    click(cx, "outer", false);
    cx.simulate_keystrokes("cmd-x");
    assert!(nothing_cut(&root, cx));
    dismiss_notices(&root, cx);

    // One open root among several selected rows refuses the whole Cut.
    click(cx, "plain", false);
    click(cx, "member", true);
    cx.simulate_keystrokes("cmd-x");
    assert!(nothing_cut(&root, cx));
    dismiss_notices(&root, cx);

    // Copy does not write its source and stays allowed.
    click(cx, "member", false);
    cx.simulate_keystrokes("cmd-c");
    assert!(nothing_cut(&root, cx));
    cx.update(|_, cx| {
        let copied = cx.read_from_clipboard().and_then(|item| item.text());
        assert!(copied.is_some_and(|text| text.ends_with("member")));
    });

    // A row that neither is nor holds an open root renames as before.
    click(cx, "plain", false);
    cx.simulate_keystrokes("shift-f6");
    cx.update(|window, cx| assert!(crate::text_field::focused(window, cx)));
    cx.simulate_keystrokes("escape");
    assert!(notices(&root, cx).is_empty());
    unmoved(cx);
}
