use guild_desktop::file_selection::FileSelection;
use std::path::PathBuf;

fn rows() -> Vec<PathBuf> {
    ["a", "b", "c", "d", "e"].map(PathBuf::from).to_vec()
}

#[test]
fn context_click_preserves_a_group_or_selects_only_the_new_target() {
    let rows = rows();
    let mut selection = FileSelection::default();
    selection.click(&rows[0], &rows, false, false);
    selection.click(&rows[2], &rows, true, false);
    selection.context_click(&rows[0], &rows);
    assert_eq!(selection.selected, vec![rows[0].clone(), rows[2].clone()]);
    selection.context_click(&rows[4], &rows);
    assert_eq!(selection.selected, vec![rows[4].clone()]);
    assert_eq!(selection.action_target(), Some(&rows[4]));
}

#[test]
fn a_partial_batch_keeps_failed_paths_and_drops_only_successful_subtrees() {
    let rows: Vec<PathBuf> = ["done", "done/child", "failed", "failed/child"]
        .map(Into::into)
        .to_vec();
    let mut selection = FileSelection::default();
    selection.key("a", &rows, true, false);
    selection.context_click(&rows[0], &rows);
    selection.remove_completed(&["done".into()]);
    assert_eq!(selection.selected, rows[2..]);
    assert!(
        selection.action_target().is_none(),
        "a successful cursor cannot silently retarget a failed item"
    );
    selection.context_click(&rows[2], &rows);
    selection.remove_completed(&[]);
    assert_eq!(selection.action_target(), Some(&rows[2]));
    assert_eq!(
        selection.selected,
        rows[2..],
        "cancellation preserves the group"
    );
}

#[test]
fn toggles_ranges_and_keyboard_share_a_stable_anchor() {
    let rows = rows();
    let mut s = FileSelection::default();
    s.click(&rows[1], &rows, false, false);
    s.click(&rows[3], &rows, true, false);
    assert_eq!(s.selected, vec![rows[1].clone(), rows[3].clone()]);
    s.click(&rows[1], &rows, true, false);
    assert_eq!(s.selected, vec![rows[3].clone()]);
    s.click(&rows[3], &rows, false, true);
    assert_eq!(s.selected, rows[1..=3]);
    s.key("up", &rows, false, true);
    assert_eq!(s.selected, rows[1..=2]);
    // Decision 54: Command arrows move the selection as the plain arrows
    // do — no cursor walks apart from it — and Command-Space is no stroke.
    s.key("down", &rows, true, false);
    assert_eq!(s.selected, rows[3..=3], "command arrows move the selection");
    assert_eq!(s.action_target(), Some(&rows[3]));
    assert!(!s.key("space", &rows, true, false));
    assert_eq!(s.selected, rows[3..=3]);
    s.key("a", &rows, true, false);
    assert_eq!(s.selected, rows);
}

#[test]
fn refresh_and_fold_remove_hidden_paths_without_retargeting_actions() {
    let rows = rows();
    let mut s = FileSelection::default();
    s.click(&rows[1], &rows, false, false);
    s.click(&rows[4], &rows, false, true);
    s.retain(&rows[..3]);
    assert_eq!(s.selected, rows[1..3]);
    assert_eq!(
        s.focus, None,
        "a removed cursor must not target another file"
    );
    s.key("down", &rows[..3], false, false);
    assert_eq!(s.selected, rows[..1]);
    s.retain(&[]);
    assert!(s.selected.is_empty());
    assert_eq!(s.focus, None);
}

#[test]
fn equal_paths_from_different_sources_stay_distinct_and_a_deselected_cursor_is_not_an_action() {
    use guild_desktop::file_selection::Selection;
    let rows = [
        (0, PathBuf::from("src/lib.rs")),
        (1, PathBuf::from("src/lib.rs")),
        (2, PathBuf::from("src/lib.rs")),
    ];
    let mut selection = Selection::default();
    selection.click(&rows[0], &rows, false, false);
    selection.click(&rows[1], &rows, true, false);
    assert_eq!(selection.selected, rows[..2]);
    selection.click(&rows[1], &rows, true, false);
    assert_eq!(selection.selected, rows[..1]);
    assert_eq!(selection.action_target(), None);
    selection.key("down", &rows, true, false);
    assert_eq!(selection.selected, rows[2..]);
    assert_eq!(selection.action_target(), Some(&rows[2]));
    selection.click(&rows[0], &rows, true, false);
    selection.click(&rows[2], &rows, true, false);
    selection.click(&rows[2], &rows, true, false);
    assert_eq!(selection.selected, [rows[0].clone(), rows[2].clone()]);
    assert_eq!(selection.action_target(), Some(&rows[2]));
    selection.retain(&rows[..1]);
    assert_eq!(selection.selected, rows[..1]);
    assert_eq!(
        selection.action_target(),
        None,
        "folding a library cannot retarget its action to the project namesake"
    );
}

#[test]
fn a_selected_project_root_copies_as_dot_alongside_ordinary_relative_paths() {
    let rows: Vec<PathBuf> = ["", "src/lib.rs"].map(Into::into).to_vec();
    let mut selection = FileSelection::default();
    selection.key("a", &rows, true, false);
    assert_eq!(selection.copied_paths(), ".\nsrc/lib.rs");
    selection.click(&rows[1], &rows, false, false);
    assert_eq!(selection.copied_paths(), "src/lib.rs");
}
