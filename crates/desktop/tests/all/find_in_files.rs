//! The Find in Files popup's rules: the toggles, the history, the scope —
//! M8.9 group C's tasks 12, 13 and 15. Declared by the seams commit, which
//! gives every file of the pass its place in `main.rs`.

use crate::scratch::Scratch;
use gpui::Keystroke;
use guild_desktop::find_in_files;
use guild_desktop::search::Toggle;

fn stroke(text: &str) -> Keystroke {
    Keystroke::parse(text).expect("a stroke parses")
}

/// **The three toggles are `alt-c`, `alt-w` and `alt-x`** — the common
/// mnemonics for *Match Case*, *Words* and *Regex* — and only with `alt`
/// alone: a letter is typed, `cmd-c` copies, and `alt-shift-c` is a
/// shortcut of somebody else's.
#[test]
fn the_three_toggles_are_alt_c_w_x() {
    assert_eq!(
        find_in_files::toggle(&stroke("alt-c")),
        Some(Toggle::MatchCase)
    );
    assert_eq!(
        find_in_files::toggle(&stroke("alt-w")),
        Some(Toggle::WholeWord)
    );
    assert_eq!(find_in_files::toggle(&stroke("alt-x")), Some(Toggle::Regex));

    for other in [
        "c",
        "w",
        "x",
        "cmd-c",
        "ctrl-w",
        "alt-shift-c",
        "alt-cmd-x",
        "alt-a",
    ] {
        assert_eq!(find_in_files::toggle(&stroke(other)), None, "{other}");
    }
}

fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

/// **What was searched is remembered newest first, once, and only so
/// many.** A query used again moves to the head rather than standing
/// twice, and the oldest falls off the end — twenty queries, ten masks.
#[test]
fn remembering_moves_an_entry_to_the_head_and_caps_the_list() {
    let mut history = list(&["b", "a"]);
    find_in_files::remember(&mut history, "c", 3);
    assert_eq!(history, list(&["c", "b", "a"]));

    find_in_files::remember(&mut history, "a", 3);
    assert_eq!(history, list(&["a", "c", "b"]), "moved, not doubled");

    find_in_files::remember(&mut history, "d", 3);
    assert_eq!(history, list(&["d", "a", "c"]), "the oldest falls off");

    assert_eq!(find_in_files::QUERIES, 20);
    assert_eq!(find_in_files::MASKS, 10);
    let mut long = Vec::new();
    for at in 0..30 {
        find_in_files::remember(&mut long, &format!("q{at}"), find_in_files::QUERIES);
    }
    assert_eq!(long.len(), 20);
    assert_eq!(long[0], "q29");
}

/// **An empty entry is not remembered**: a search of nothing is not a
/// search, and a mask of nothing is every file, which is no mask to go
/// back to.
#[test]
fn an_empty_entry_is_not_remembered() {
    let mut history = list(&["a"]);
    find_in_files::remember(&mut history, "", 20);
    assert_eq!(history, list(&["a"]));
}

/// **A mask that is off is not applied, and it is kept.** A person who
/// unticks `*.rs` to look everywhere for a moment ticks it back; finding
/// the field empty would be the work thrown away. The walk is asked
/// without it, and the field still holds it.
#[test]
fn a_mask_that_is_off_is_not_applied_and_is_kept() {
    let query = guild_desktop::search::Query {
        text: "needle".to_owned(),
        case_sensitive: true,
        mask: "*.rs".to_owned(),
        scope: Some("crates/a".into()),
        ..Default::default()
    };

    let off = find_in_files::query_for(&query, true);
    assert_eq!(off.mask, "", "not applied");
    assert_eq!(off.text, "needle");
    assert!(off.case_sensitive);
    assert_eq!(off.scope, query.scope, "nothing else changes");
    assert_eq!(query.mask, "*.rs", "and kept where it was typed");

    assert_eq!(find_in_files::query_for(&query, false), query);
}

/// **The Search tool keeps a mask the popup turned off.** The tool has no
/// checkbox, so it walks without the mask and writes an empty one; writing
/// that over the popup's `*.rs` would empty the popup's field the next
/// time it opens. A mask typed in the tool is the tool's, and is on.
#[test]
fn the_tool_keeps_a_mask_the_popup_turned_off() {
    assert_eq!(
        find_in_files::tool_mask("*.rs", true, ""),
        ("*.rs".to_owned(), true)
    );
    assert_eq!(
        find_in_files::tool_mask("*.rs", true, "*.md"),
        ("*.md".to_owned(), false)
    );
    assert_eq!(
        find_in_files::tool_mask("*.rs", false, ""),
        (String::new(), false)
    );
    assert_eq!(
        find_in_files::tool_mask("", false, "*.toml"),
        ("*.toml".to_owned(), false)
    );
}

/// **`alt-up` walks back through what was searched, and `alt-down` comes
/// back to what was being typed.** The walk stops at the oldest entry, and
/// past the newest there is only the line as it was before the first
/// `alt-up`.
#[test]
fn alt_up_walks_back_through_the_history_and_alt_down_returns() {
    let history = list(&["greet", "needle_one", "alpha"]);
    let mut recall = find_in_files::Recall::default();

    assert_eq!(recall.newer(&history), None, "nothing to come back to yet");
    assert_eq!(recall.older(&history, "typed"), Some("greet".to_owned()));
    assert_eq!(
        recall.older(&history, "greet"),
        Some("needle_one".to_owned())
    );
    assert_eq!(
        recall.older(&history, "needle_one"),
        Some("alpha".to_owned())
    );
    assert_eq!(
        recall.older(&history, "alpha"),
        None,
        "the oldest is the end"
    );
    assert_eq!(recall.newer(&history), Some("needle_one".to_owned()));
    assert_eq!(recall.newer(&history), Some("greet".to_owned()));
    assert_eq!(
        recall.newer(&history),
        Some("typed".to_owned()),
        "what was typed"
    );
    assert_eq!(recall.newer(&history), None);

    assert_eq!(
        find_in_files::Recall::default().older(&[], "typed"),
        None,
        "no history is nowhere to go"
    );
}

/// **A file row scopes the search to its directory, a directory row to
/// itself** — the *Directory* chip and M8.8.3's *Find in Files…* on a
/// row, one rule for both.
#[test]
fn a_file_row_scopes_to_its_directory_and_a_directory_to_itself() {
    use find_in_files::Scope;
    use std::path::{Path, PathBuf};

    assert_eq!(
        find_in_files::scope_of_row(Path::new("crates/a/src/lib.rs"), false),
        Scope::Directory(PathBuf::from("crates/a/src"))
    );
    assert_eq!(
        find_in_files::scope_of_row(Path::new("crates/a"), true),
        Scope::Directory(PathBuf::from("crates/a"))
    );
}

/// **The project's own row is the project** — the trap *a menu sees rows,
/// and one row is not a row*: the Explorer draws the project's directory as
/// a row with the empty path, and a file at the top of the project has the
/// empty path for its directory. A chip saying *Directory:* and nothing is
/// a narrowing that narrows nothing.
#[test]
fn the_project_s_own_row_is_the_project() {
    use find_in_files::Scope;
    use std::path::Path;

    assert_eq!(
        find_in_files::scope_of_row(Path::new(""), true),
        Scope::Project
    );
    assert_eq!(
        find_in_files::scope_of_row(Path::new("Cargo.toml"), false),
        Scope::Project
    );
    assert_eq!(Scope::Project.relative(), None);
    assert_eq!(
        Scope::Directory("web".into()).relative(),
        Some(Path::new("web"))
    );
}

/// A directory of our own under the temporary directory, named after the
/// test and this process.
fn scratch(name: &str) -> Scratch {
    let root = Scratch::new(&format!("guild-fif-{name}-"));
    std::fs::create_dir_all(root.join("project/crates/a")).expect("a directory");
    std::fs::create_dir_all(root.join("elsewhere")).expect("a directory");
    root
}

/// **A folder chosen outside the project is refused**: a search's hits are
/// the project's paths, and a folder beside it has none. So is a folder
/// that is not there, and the project itself is the project.
#[test]
fn a_chosen_folder_outside_the_project_is_refused() {
    use find_in_files::Scope;

    let root = scratch("outside");
    let project = root.join("project");

    assert_eq!(
        find_in_files::scope_of_chosen(&project, &root.join("elsewhere")),
        None
    );
    assert_eq!(
        find_in_files::scope_of_chosen(&project, &root),
        None,
        "a parent"
    );
    assert_eq!(
        find_in_files::scope_of_chosen(&project, &root.join("gone")),
        None
    );
    assert_eq!(
        find_in_files::scope_of_chosen(&project, &root.join("project-two")),
        None,
        "a name that starts the same is not inside"
    );
    assert_eq!(
        find_in_files::scope_of_chosen(&project, &project),
        Some(Scope::Project)
    );
    assert_eq!(
        find_in_files::scope_of_chosen(&project, &project.join("crates/a")),
        Some(Scope::Directory("crates/a".into()))
    );
}

/// **A folder under a symlinked temporary directory is inside** — `/var` is
/// `/private/var` on this platform, the chooser answers the real path and a
/// project may have been opened by the link. Both are made canonical before
/// they are compared.
#[test]
fn a_chosen_folder_under_a_symlinked_temp_directory_is_inside() {
    use find_in_files::Scope;

    let root = scratch("symlinked");
    let project = root.join("project");
    let real = project.canonicalize().expect("canonical");
    let link = root.join("link");
    std::os::unix::fs::symlink(&project, &link).expect("a symlink");

    assert_eq!(
        find_in_files::scope_of_chosen(&link, &real.join("crates/a")),
        Some(Scope::Directory("crates/a".into())),
        "the project by its link, the folder by its real path"
    );
    assert_eq!(
        find_in_files::scope_of_chosen(&real, &link.join("crates/a")),
        Some(Scope::Directory("crates/a".into())),
        "and the other way round"
    );
}
