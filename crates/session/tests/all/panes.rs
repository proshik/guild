// crates/session/tests/all/panes.rs
//! A tab's panes — M12.8. `Panes<B>` is the same model for what the state
//! file writes (`Shows`) and what the window holds (`Body`); here `B` is a
//! word.

use guild_session::layout::Axis;
use guild_session::panes::{Closed, Panes};

#[test]
fn a_tab_starts_with_one_pane_showing_its_body() {
    let panes = Panes::one("claude");
    assert_eq!(panes.ids().len(), 1);
    assert_eq!(panes.body(), Some(&"claude"));
}

/// Decision 13: a new pane is empty and takes the focus.
#[test]
fn a_new_pane_is_empty_and_takes_the_focus() {
    let mut panes = Panes::one("claude");
    let first = panes.focused();
    let second = panes
        .split(first, Axis::Horizontal)
        .expect("the pane is there");
    assert_eq!(panes.focused(), second);
    assert_eq!(panes.body(), None);
    assert_eq!(panes.body_of(first), Some(&"claude"));
    assert_eq!(panes.showing(), 1);
}

#[test]
fn an_empty_pane_splits_again() {
    let mut panes = Panes::one("claude");
    let second = panes.split(panes.focused(), Axis::Horizontal).unwrap();
    let third = panes.split(second, Axis::Vertical).unwrap();
    assert_eq!(panes.ids().len(), 3);
    assert_eq!(panes.focused(), third);
    assert_eq!(panes.showing(), 1);
}

#[test]
fn an_empty_pane_is_filled_once() {
    let mut panes = Panes::one("claude");
    let second = panes.split(panes.focused(), Axis::Horizontal).unwrap();
    assert_eq!(panes.fill(second, "shell"), Ok(()));
    assert_eq!(panes.body(), Some(&"shell"));
    assert_eq!(
        panes.fill(second, "again"),
        Err("again"),
        "a pane that shows something is kept"
    );
}

#[test]
fn closing_a_pane_gives_back_its_body_and_the_focus_to_its_heir() {
    let mut panes = Panes::one("claude");
    let first = panes.focused();
    let second = panes.split(first, Axis::Horizontal).unwrap();
    panes.fill(second, "shell").unwrap();
    assert_eq!(panes.close(second), Closed::Pane(Some("shell")));
    assert_eq!(panes.ids(), vec![first]);
    assert_eq!(panes.focused(), first);
}

/// Invariant 3: the tab closes with its last pane — the model says so and
/// changes nothing, because closing the tab is the window's.
#[test]
fn closing_the_last_pane_closes_the_tab() {
    let mut panes = Panes::one("claude");
    let only = panes.focused();
    assert_eq!(panes.close(only), Closed::Tab);
    assert_eq!(panes.ids(), vec![only]);
}

#[test]
fn closing_the_last_pane_that_shows_anything_closes_the_tab() {
    let mut panes = Panes::one("claude");
    let first = panes.focused();
    panes.split(first, Axis::Horizontal).unwrap();
    assert_eq!(panes.close(first), Closed::Tab);
    assert_eq!(panes.ids().len(), 2, "nothing moved");
}

#[test]
fn a_body_is_found_in_whichever_pane_shows_it() {
    let mut panes = Panes::one("claude");
    let first = panes.focused();
    let second = panes.split(first, Axis::Horizontal).unwrap();
    panes.fill(second, "lib.rs").unwrap();
    assert!(panes.focus(first));
    assert_eq!(panes.find(|body| *body == "lib.rs"), Some(second));
    assert_eq!(panes.find(|body| *body == "gone.rs"), None);
}

#[test]
fn a_body_can_be_taken_out_leaving_its_pane_empty() {
    let mut panes = Panes::one("claude");
    let only = panes.focused();
    assert_eq!(panes.take(only), Some("claude"));
    assert_eq!(panes.body(), None);
}

/// What `store_tabs` needs: a body that cannot be written loses its pane,
/// an empty pane stays empty, and nothing written is no tab.
#[test]
fn mapping_drops_what_cannot_be_written_and_keeps_empty_panes() {
    let mut panes = Panes::one("claude");
    let first = panes.focused();
    let second = panes.split(first, Axis::Horizontal).unwrap();
    panes.fill(second, "proposal").unwrap();
    let third = panes.split(second, Axis::Vertical).unwrap();
    let written = panes
        .map(|body| (*body != "proposal").then(|| body.to_uppercase()))
        .expect("claude is left");
    assert_eq!(written.ids(), vec![first, third]);
    assert_eq!(written.body_of(first), Some(&"CLAUDE".to_owned()));
    assert_eq!(written.body_of(third), None);
    assert!(Panes::one("proposal").map(|_| None::<String>).is_none());
}

/// What a restore needs: the shape without its bodies, to pour into.
#[test]
fn an_emptied_copy_keeps_the_shape_and_the_focus() {
    let mut panes = Panes::one("claude");
    let second = panes.split(panes.focused(), Axis::Horizontal).unwrap();
    let shape: Panes<u32> = panes.emptied();
    assert_eq!(shape.ids(), panes.ids());
    assert_eq!(shape.focused(), second);
    assert_eq!(shape.showing(), 0);
}

/// A hand-edited file cannot name a pane that is not in the tree.
#[test]
fn a_file_s_stray_body_and_focus_are_put_right() {
    let mut panes = Panes::one("claude");
    let first = panes.focused();
    let second = panes.split(first, Axis::Horizontal).unwrap();
    let mut value = serde_json::to_value(&panes).unwrap();
    value["focused"] = serde_json::json!(uuid::Uuid::new_v4());
    value["showing"][uuid::Uuid::new_v4().to_string()] = serde_json::json!("stray");
    let read: Panes<String> = serde_json::from_value(value).unwrap();
    let read = read.normalised();
    assert_eq!(read.focused(), first);
    assert_eq!(read.showing(), 1);
    assert_eq!(read.ids(), vec![first, second]);
}

/// A stale id must not close the tab: it is not a pane of this one.
#[test]
fn closing_a_pane_that_is_not_here_changes_nothing() {
    let mut panes = Panes::one("claude");
    let first = panes.focused();
    let second = panes.split(first, Axis::Horizontal).unwrap();
    let foreign = Panes::one("other").focused();
    assert_eq!(panes.close(foreign), Closed::Missing);
    assert_eq!(panes.ids(), vec![first, second]);
    assert_eq!(panes.focused(), second);
    assert_eq!(panes.showing(), 1);
}

#[test]
fn extracting_owned_panes_keeps_ids_bodies_and_unrelated_empty_places() {
    let mut panes = Panes::one("origin".to_owned());
    let parent = panes.focused();
    let unrelated = panes.split(parent, Axis::Horizontal).unwrap();
    panes.fill(unrelated, "unrelated".to_owned()).unwrap();
    let process = panes.split(parent, Axis::Vertical).unwrap();
    panes.fill(process, "process".to_owned()).unwrap();
    let empty = panes.split(unrelated, Axis::Vertical).unwrap();
    panes.focus(unrelated);
    let extracted = panes.extract(&[parent, process]).unwrap();
    assert_eq!(extracted.ids(), vec![parent, process]);
    assert_eq!(extracted.body_of(parent).unwrap().as_str(), "origin");
    assert_eq!(extracted.body_of(process).unwrap().as_str(), "process");
    assert_eq!(panes.ids(), vec![unrelated, empty]);
    assert_eq!(panes.focused(), unrelated);
    assert_eq!(panes.body().unwrap().as_str(), "unrelated");
    assert_eq!(extracted.focused(), parent);
    let unchanged = panes.clone();
    assert!(panes.extract(&[unrelated]).is_none());
    assert_eq!(panes, unchanged);
    let saved = serde_json::to_vec(&extracted).unwrap();
    let restored: Panes<String> = serde_json::from_slice(&saved).unwrap();
    assert_eq!(restored, extracted);
}
