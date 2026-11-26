//! Push Commits, the window — M8.3.6: what it offers and what it keeps.

use guild_desktop::push_dialog::{Selection, can_push, comparison, refusal, valid_target};

#[test]
fn push_is_offered_only_when_there_is_something_and_nothing_runs() {
    assert!(can_push(2, true, false));
    assert!(!can_push(0, true, false), "nothing to push");
    assert!(!can_push(2, false, false), "nowhere to push to");
    assert!(!can_push(2, true, true), "a push is already out");
}

#[test]
fn the_branch_is_selected_on_opening_and_a_landed_list_keeps_a_commit_selection() {
    let shas = ["c2", "c1"];
    let mut selected = Selection::default();
    assert!(selected.shas.is_empty(), "the branch shows all changes");
    assert_eq!(comparison(&selected.shas, &shas).as_deref(), Some("c2,c1"));
    selected.click("c1", &shas, false, false);
    selected.retain(&shas);
    assert_eq!(selected.shas, ["c1"], "kept while it is still there");
    selected.retain(&["c2"]);
    assert!(
        selected.shas.is_empty(),
        "a disappeared commit gives way to the branch"
    );
    assert_eq!(comparison(&selected.shas, &[]), None, "nothing waits");
}

#[test]
fn command_toggles_commits_and_shift_extends_from_the_anchor_in_log_order() {
    let shas = ["c4", "c3", "c2", "c1"];
    let mut selected = Selection::default();
    selected.click("c2", &shas, false, false);
    selected.click("c4", &shas, true, false);
    assert_eq!(
        comparison(&selected.shas, &shas).as_deref(),
        Some("c4,c2"),
        "no skipped commit is inspected"
    );
    selected.click("c4", &shas, true, false);
    assert_eq!(selected.shas, ["c2"]);
    selected.click("c1", &shas, false, true);
    assert_eq!(selected.shas, shas, "Shift spans from the last clicked row");
    selected.click("c3", &shas, false, false);
    assert_eq!(
        selected.shas,
        ["c3"],
        "a plain click replaces the selection"
    );
}

#[test]
fn a_target_is_a_branch_name_and_cannot_change_the_push_refspec() {
    for name in ["main", "release/2026.10", "fix/push-dialog", "ветка"] {
        assert!(valid_target(name), "{name}");
    }
    for name in [
        "",
        "@",
        "--all",
        "main:other",
        ":main",
        "+main",
        "a..b",
        "a~1",
        "refs/*",
        "HEAD@{1}",
        "a b",
        "a\n",
        "a//b",
        ".hidden",
        "a/.hidden",
        "a.lock/b",
        "a/",
    ] {
        assert!(!valid_target(name), "{name:?}");
    }
}

fn repository() -> crate::scratch::Scratch {
    let folder = crate::scratch::Scratch::new("guild-push-inspection-");
    guild_desktop::git::run(&folder, &["init", "-q", "-b", "main"]).unwrap();
    guild_desktop::git::run(&folder, &["config", "user.name", "Guild"]).unwrap();
    guild_desktop::git::run(&folder, &["config", "user.email", "guild@example.invalid"]).unwrap();
    folder
}

fn commit(folder: &std::path::Path, message: &str) -> String {
    guild_desktop::git::run(folder, &["add", "--all"]).unwrap();
    guild_desktop::git::run(folder, &["commit", "-q", "-m", message]).unwrap();
    String::from_utf8(guild_desktop::git::run(folder, &["rev-parse", "HEAD"]).unwrap())
        .unwrap()
        .trim()
        .to_owned()
}

#[test]
fn nonadjacent_commits_keep_only_selected_files_and_each_files_own_blobs() {
    use guild_desktop::git::{self, inspection};
    let folder = repository();
    std::fs::write(folder.join("a.txt"), "selected first\n").unwrap();
    let first = commit(&folder, "first");
    std::fs::write(folder.join("a.txt"), "unselected edit\n").unwrap();
    std::fs::write(folder.join("skipped.txt"), "unselected file\n").unwrap();
    commit(&folder, "skipped");
    std::fs::write(folder.join("c.txt"), "selected last\n").unwrap();
    let last = commit(&folder, "last");
    let key = comparison(&[first.clone(), last.clone()], &[&last, &first]).unwrap();
    let changes = inspection::read(&folder, &key).unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].file.path, std::path::Path::new("a.txt"));
    assert_eq!(
        changes[0].after.as_ref().unwrap().0,
        first,
        "the skipped edit is not the after blob"
    );
    let files = git::log::files_of(&git::commit_files(&folder, &key).unwrap()).unwrap();
    assert_eq!(
        files,
        changes.iter().map(|c| c.file.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn combined_changes_follow_renames_and_remove_a_file_added_then_deleted() {
    use guild_desktop::git::{self, inspection};
    use guild_desktop::theme::Mark;
    let folder = repository();
    std::fs::write(folder.join("old.txt"), "original contents\n").unwrap();
    let base = commit(&folder, "base");
    std::fs::rename(folder.join("old.txt"), folder.join("new.txt")).unwrap();
    std::fs::write(folder.join("temporary.txt"), "temporary\n").unwrap();
    let first = commit(&folder, "rename and add");
    std::fs::write(folder.join("new.txt"), "original contents\nsecond line\n").unwrap();
    std::fs::remove_file(folder.join("temporary.txt")).unwrap();
    let last = commit(&folder, "modify and delete");
    let key = comparison(&[], &[&last, &first]).unwrap();
    let changes = inspection::read(&folder, &key).unwrap();
    assert_eq!(changes.len(), 1);
    let change = &changes[0];
    assert_eq!(change.file.mark, Mark::Modified);
    assert_eq!(
        change.file.from.as_deref(),
        Some(std::path::Path::new("old.txt"))
    );
    assert_eq!(change.file.path, std::path::Path::new("new.txt"));
    let (sha, path) = change.before.as_ref().unwrap();
    assert_eq!(
        git::blob(&folder, &format!("{sha}:{}", path.display())).unwrap(),
        git::blob(&folder, &format!("{base}:old.txt")).unwrap()
    );
    assert_eq!(change.after.as_ref().unwrap().0, last);
}

/// **A refused push is said in git's words, without its hints** — they
/// run to five lines and, in a window, are advice read as an instruction.
/// The *To …* line names the remote the head already names.
#[test]
fn a_refusal_keeps_git_s_verdict_and_drops_its_hints() {
    let words = "To /work/remote.git\n ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs to '/work/remote.git'\nhint: Updates were rejected because the remote contains work that you do not\nhint: have locally.\n";
    assert_eq!(
        refusal(words),
        "! [rejected]        main -> main (fetch first)\nerror: failed to push some refs to '/work/remote.git'"
    );
    assert_eq!(refusal("fatal: no such remote"), "fatal: no such remote");
}
