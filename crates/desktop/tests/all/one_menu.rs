//! One menu for the whole window — M12.7.

use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir`, walked by hand: there is no `walkdir`
/// crate in section 11, and ten lines do not need one.
fn walkdir(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(walkdir(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
    found
}

/// Whether `text` names `name` as a whole identifier.
///
/// **Not a substring**: a note may cite a name such as
/// `ProjectViewPopupMenu` or `ChangesViewPopupMenu`, which is not the
/// component. Each of the component's names
/// is listed on its own, so a whole-identifier match still catches
/// `PopupMenuItem` beside `PopupMenu`.
fn names(text: &str, name: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(ident) && !after.is_some_and(ident)
    })
}

/// No view of ours draws the component's menu: it fixes its rows at 26
/// and lights two of them at once — M12.7.
#[test]
fn no_view_draws_the_component_s_menu() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/view");
    let files = walkdir(&root);
    assert!(
        !files.is_empty(),
        "the walk found no views under {}",
        root.display()
    );
    for entry in files {
        let text = std::fs::read_to_string(&entry).unwrap();
        for name in [
            "PopupMenu",
            "DropdownMenu",
            "dropdown_menu",
            "PopupMenuItem",
        ] {
            assert!(
                !names(&text, name),
                "{} still names {name}",
                entry.display()
            );
        }
    }
}

/// The match above is by identifier, so it is checked against the shapes
/// it must catch and the ones it must let by.
#[test]
fn the_guard_catches_the_component_s_names_and_not_others() {
    assert!(names(
        "use gpui_component::menu::{DropdownMenu as _, PopupMenu};",
        "PopupMenu"
    ));
    assert!(names("PopupMenuItem::new(x)", "PopupMenuItem"));
    assert!(names("button.dropdown_menu(f)", "dropdown_menu"));
    assert!(!names("the `ProjectViewPopupMenu` name", "PopupMenu"));
    assert!(!names("PopupMenuItem::new(x)", "PopupMenu"));
}
