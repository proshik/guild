//! The Changes panel's three groups, built from what `git status` said.
//! On the captured porcelain of `tests/git_status.rs`, never a
//! repository: the fixture has every case the parser knows, so it has
//! every case a group can hold.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use guild_desktop::changes::{Group, GroupKind, groups};
use guild_desktop::git::status::{StatusMap, parse};
use guild_desktop::theme::Mark;

#[test]
fn selected_directory_inclusion_stages_only_selected_eligible_children() {
    use guild_desktop::changes::commit_plan;
    let map = StatusMap::from_entries(parse(b"? new/\0").unwrap());
    let ticks = HashSet::from([PathBuf::from("new/one.txt")]);
    assert_eq!(
        commit_plan(&map, &ticks, &HashSet::new()).stage,
        vec![PathBuf::from("new/one.txt")]
    );
}

#[test]
fn directory_path_itself_never_becomes_a_commit_pathspec() {
    let map = StatusMap::from_entries(parse(b"? new/\0").unwrap());
    assert!(
        guild_desktop::changes::commit_plan(&map, &HashSet::from(["new/".into()]), &HashSet::new())
            .stage
            .is_empty()
    );
}

#[test]
fn eligibility_is_atomic_for_conflicts_loading_errors_and_empty_directories() {
    use guild_desktop::changes::{Directory, eligible_targets, list};
    use std::collections::HashMap;
    let map = StatusMap::from_entries(parse(b"? a.txt\0? new/\0").unwrap());
    let rows = list(&map);
    let selection = vec!["a.txt".into(), "new/".into()];
    assert!(
        eligible_targets(&rows, &HashMap::new(), &selection)
            .unwrap_err()
            .to_string()
            .contains("loading")
    );
    let mut dirs = HashMap::from([("new/".into(), Err("Permission denied".into()))]);
    assert!(
        eligible_targets(&rows, &dirs, &selection)
            .unwrap_err()
            .to_string()
            .contains("Permission denied")
    );
    dirs.insert(
        "new/".into(),
        Ok(Directory {
            files: vec![],
            ignored: 3,
            ..Default::default()
        }),
    );
    assert!(
        eligible_targets(&rows, &dirs, &selection)
            .unwrap_err()
            .to_string()
            .contains("no eligible")
    );
    dirs.insert(
        "new/".into(),
        Ok(Directory {
            files: vec!["new/b.txt".into()],
            ignored: 3,
            ..Default::default()
        }),
    );
    assert_eq!(
        eligible_targets(&rows, &dirs, &selection).unwrap(),
        vec![PathBuf::from("a.txt"), "new/b.txt".into()]
    );
    let mut conflicted = rows.clone();
    conflicted
        .iter_mut()
        .find(|l| l.row.path == Path::new("a.txt"))
        .unwrap()
        .row
        .unmerged = true;
    assert!(
        eligible_targets(&conflicted, &dirs, &selection)
            .unwrap_err()
            .to_string()
            .contains("unresolved conflicts")
    );
    assert_eq!(
        eligible_targets(&rows, &dirs, &["new/".into(), "new/b.txt".into()]).unwrap(),
        vec![PathBuf::from("new/b.txt")],
        "a selected parent subsumes its descendants before resolving targets"
    );
}

#[test]
fn directory_discovery_uses_git_ignore_rules_and_preserves_unusual_names() {
    use guild_desktop::changes::discover_directory;
    let dir = tempfile::tempdir().unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success()
    );
    std::fs::create_dir(dir.path().join("new")).unwrap();
    std::fs::write(dir.path().join(".gitignore"), "*.ignored\n").unwrap();
    for name in [
        "hello world.txt",
        "кириллица.txt",
        "hidden.ignored",
        ".hidden.txt",
    ] {
        std::fs::write(dir.path().join("new").join(name), "data").unwrap();
    }
    let snapshot = discover_directory(dir.path(), Path::new("new")).unwrap();
    assert_eq!(snapshot.files.len(), 3);
    assert_eq!(snapshot.ignored, 1);
    assert!(snapshot.files.contains(&PathBuf::from("new/.hidden.txt")));
    assert!(
        !snapshot
            .files
            .iter()
            .any(|p| p.extension().is_some_and(|e| e == "ignored"))
    );
}

const CAPTURED: &[u8] = include_bytes!("../fixtures/porcelain-v2.bin");

fn captured() -> Vec<Group> {
    groups(&StatusMap::from_entries(
        parse(CAPTURED).expect("porcelain v2"),
    ))
}

fn group(groups: &[Group], kind: GroupKind) -> &Group {
    groups
        .iter()
        .find(|g| g.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} group"))
}

fn paths(group: &Group) -> Vec<&Path> {
    group.rows.iter().map(|row| row.path.as_path()).collect()
}

/// Three groups, always, in git's order: what a commit would take, what
/// it would not, and what git does not know about.
#[test]
fn the_three_groups_come_in_order_and_count_their_rows() {
    let groups = captured();
    let kinds: Vec<_> = groups.iter().map(|g| g.kind).collect();
    assert_eq!(
        kinds,
        [GroupKind::Staged, GroupKind::Unstaged, GroupKind::Untracked]
    );
    assert_eq!(groups[0].rows.len(), 5, "{:?}", paths(&groups[0]));
    assert_eq!(groups[1].rows.len(), 5, "{:?}", paths(&groups[1]));
    assert_eq!(groups[2].rows.len(), 2, "{:?}", paths(&groups[2]));
}

/// The staged group is the index against `HEAD`: the added, the
/// deleted, the renamed with its old name, and the modified — in path
/// order, each with the mark the tree draws.
#[test]
fn the_staged_group_is_what_a_commit_would_take() {
    let groups = captured();
    let staged = group(&groups, GroupKind::Staged);
    assert_eq!(
        paths(staged),
        [
            Path::new("src/added.rs"),
            Path::new("src/both.rs"),
            Path::new("src/deleted.rs"),
            Path::new("src/new-name.rs"),
            Path::new("src/staged.rs"),
        ]
    );
    let marks: Vec<_> = staged.rows.iter().map(|row| row.mark).collect();
    assert_eq!(
        marks,
        [
            Mark::Added,
            Mark::Modified,
            Mark::Deleted,
            Mark::Modified,
            Mark::Modified
        ]
    );
    let renamed = &staged.rows[3];
    assert_eq!(
        renamed.from.as_deref(),
        Some(Path::new("src/old-name.rs")),
        "a rename is one row, and it says where from"
    );
    assert!(
        staged.rows.iter().filter(|r| r.from.is_some()).count() == 1,
        "only the rename has an old name"
    );
}

/// The unstaged group is the working tree against the index. A file
/// changed on both sides is in both groups; an unmerged file is here and
/// not in the staged group, because a commit cannot take a conflict.
#[test]
fn the_unstaged_group_is_what_a_commit_would_leave() {
    let groups = captured();
    let unstaged = group(&groups, GroupKind::Unstaged);
    assert_eq!(
        paths(unstaged),
        [
            Path::new("docs/файл.md"),
            Path::new("src/both.rs"),
            Path::new("src/conflict.rs"),
            Path::new("src/modified.rs"),
            Path::new("with space/file name.md"),
        ]
    );
    assert!(unstaged.rows.iter().all(|row| row.mark == Mark::Modified));
    assert!(unstaged.rows.iter().all(|row| row.from.is_none()));
    let staged = group(&groups, GroupKind::Staged);
    assert!(
        !paths(staged).contains(&Path::new("src/conflict.rs")),
        "a conflict is not staged"
    );
}

/// The untracked group holds files and the directories git did not
/// descend into, each once; what an ignore rule names is in no group.
#[test]
fn the_untracked_group_has_files_and_directories_and_nothing_ignored() {
    let groups = captured();
    let untracked = group(&groups, GroupKind::Untracked);
    assert_eq!(
        paths(untracked),
        [Path::new("newdir"), Path::new("untracked.txt")]
    );
    assert!(untracked.rows[0].is_dir && !untracked.rows[1].is_dir);
    assert!(untracked.rows.iter().all(|row| row.mark == Mark::Untracked));
    let everywhere: Vec<PathBuf> = groups
        .iter()
        .flat_map(|g| g.rows.iter().map(|r| r.path.clone()))
        .collect();
    assert!(!everywhere.contains(&PathBuf::from("build.log")));
    assert!(!everywhere.contains(&PathBuf::from("target")));
}

/// A clean repository is three empty groups, not none: the panel draws
/// the same shape with nothing in it.
#[test]
fn a_clean_tree_is_three_empty_groups() {
    let groups = groups(&StatusMap::default());
    assert_eq!(groups.len(), 3);
    assert!(groups.iter().all(|g| g.rows.is_empty()));
    assert_eq!(groups.iter().map(|g| g.rows.len()).sum::<usize>(), 0);
}

/// The rows walk in the panel's order, and a row's line counts the
/// heads above it — the second section's first row is not line 1.
#[test]
fn rows_walk_in_order_and_a_line_counts_the_heads() {
    use guild_desktop::changes::{Section, line_of, list, rows_in_order};
    let listed = list(&StatusMap::from_entries(
        parse(CAPTURED).expect("porcelain v2"),
    ));
    let rows = rows_in_order(&listed);
    assert_eq!(rows.len(), 9 + 2);
    assert_eq!(rows[0].section, Section::Changes);
    assert_eq!(rows[9].section, Section::Unversioned);
    assert_eq!(
        line_of(&listed, Section::Changes, Path::new("docs/файл.md")),
        Some(1)
    );
    assert_eq!(
        line_of(&listed, Section::Unversioned, Path::new("newdir")),
        Some(11),
        "nine changes, their head, and the unversioned head"
    );
    assert_eq!(
        line_of(&listed, Section::Unversioned, Path::new("docs/файл.md")),
        None
    );
}

/// The message file holds the message as typed — quotes, a blank line,
/// a `#` line — with one newline at the end, so that git reads the
/// summary and the body and nothing is escaped on the way.
#[test]
fn the_message_file_is_the_message_unchanged_with_a_trailing_newline() {
    use guild_desktop::changes::message_file_contents;
    let typed = "Fix \"the\" thing\n\n#42 is why\n  indented";
    assert_eq!(message_file_contents(typed), format!("{typed}\n"));
    assert_eq!(message_file_contents("one line\n"), "one line\n");
    assert_eq!(message_file_contents("one line\n\n\n"), "one line\n");
}

/// One row per tracked file whatever git has staged of it, then the
/// unversioned files; the ignored in neither. Decision 4
/// of M8.3.1.
#[test]
fn the_list_is_one_row_per_change_and_the_unversioned_after() {
    use guild_desktop::changes::{Section, list};
    let listed = list(&StatusMap::from_entries(
        parse(CAPTURED).expect("porcelain v2"),
    ));
    let changes: Vec<&Path> = listed
        .iter()
        .filter(|l| l.section == Section::Changes)
        .map(|l| l.row.path.as_path())
        .collect();
    assert_eq!(
        changes,
        [
            Path::new("docs/файл.md"),
            Path::new("src/added.rs"),
            Path::new("src/both.rs"),
            Path::new("src/conflict.rs"),
            Path::new("src/deleted.rs"),
            Path::new("src/modified.rs"),
            Path::new("src/new-name.rs"),
            Path::new("src/staged.rs"),
            Path::new("with space/file name.md"),
        ],
        "a file changed on both sides is one row"
    );
    let unversioned: Vec<&Path> = listed
        .iter()
        .filter(|l| l.section == Section::Unversioned)
        .map(|l| l.row.path.as_path())
        .collect();
    assert_eq!(
        unversioned,
        [Path::new("newdir"), Path::new("untracked.txt")]
    );
    let renamed = listed
        .iter()
        .find(|l| l.row.path == Path::new("src/new-name.rs"))
        .unwrap();
    assert_eq!(
        renamed.row.from.as_deref(),
        Some(Path::new("src/old-name.rs")),
        "a staged rename still says where from"
    );
}

/// The plan brings the index to the ticks: a ticked row not fully staged
/// is added, a staged row left unticked is taken back out — a rename
/// with both of its paths — and a ticked unversioned file is added. A
/// ticked row already staged needs nothing.
#[test]
fn the_commit_plan_brings_the_index_to_the_ticks() {
    use guild_desktop::changes::commit_plan;
    let map = StatusMap::from_entries(parse(CAPTURED).expect("porcelain v2"));
    let ticked: HashSet<PathBuf> = [
        "src/modified.rs",
        "src/both.rs",
        "src/staged.rs",
        "untracked.txt",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    let plan = commit_plan(&map, &ticked, &HashSet::new());
    assert_eq!(
        plan.stage,
        [
            PathBuf::from("src/both.rs"),
            PathBuf::from("src/modified.rs"),
            PathBuf::from("untracked.txt"),
        ]
    );
    assert_eq!(
        plan.unstage,
        [
            PathBuf::from("src/added.rs"),
            PathBuf::from("src/deleted.rs"),
            PathBuf::from("src/new-name.rs"),
            PathBuf::from("src/old-name.rs"),
        ],
        "a staged rename is taken back with both of its paths; the conflict is left alone"
    );
    let with_conflict: HashSet<PathBuf> = [PathBuf::from("src/conflict.rs")].into_iter().collect();
    let plan = commit_plan(&map, &with_conflict, &HashSet::new());
    assert!(
        !plan.stage.contains(&PathBuf::from("src/conflict.rs")),
        "a ticked conflict is not added: git add would call it resolved"
    );
}

/// Nothing ticked is nothing to commit; a blank message is no message.
#[test]
fn a_commit_needs_a_tick_and_a_message() {
    use guild_desktop::changes::can_commit;
    let one: HashSet<PathBuf> = [PathBuf::from("a.rs")].into_iter().collect();
    let none = HashSet::new();
    assert!(can_commit(&one, &none, "Fix"));
    assert!(!can_commit(&HashSet::new(), &none, "Fix"));
    assert!(!can_commit(&one, &none, " \n"));
}

/// A row of the Commit panel as `changes::list` builds it, with the two
/// facts `to_tick` reads: whether the index has it, and whether it is a
/// conflict.
fn row_of(
    section: guild_desktop::changes::Section,
    path: &str,
    staged: bool,
    unmerged: bool,
) -> guild_desktop::changes::Listed {
    use guild_desktop::changes::{Listed, Row, Section};
    Listed {
        section,
        row: Row {
            path: PathBuf::from(path),
            mark: if section == Section::Unversioned {
                Mark::Untracked
            } else {
                Mark::Modified
            },
            from: None,
            is_dir: false,
            unmerged,
            staged,
        },
    }
}

/// **A file an agent `git add`ed arrives ticked.** It was unversioned when
/// the panel listed it — unticked, as every unversioned file is — and is
/// a staged change now. Left unticked, the commit would unstage it:
/// decision 16.
#[test]
fn a_file_staged_in_a_terminal_after_it_was_listed_arrives_ticked() {
    use guild_desktop::changes::{Section, to_tick};
    let before = [row_of(Section::Unversioned, "notes.md", false, false)];
    let now = [row_of(Section::Changes, "notes.md", true, false)];
    assert_eq!(to_tick(&before, &now), [PathBuf::from("notes.md")]);
}

/// A change the person unticked and then staged by hand — in a terminal,
/// or with *Add to VCS* — is ticked again: nothing in the window stages a
/// tracked change on its own, so an index side that became staged was
/// staged by a hand that meant it.
#[test]
fn a_change_staged_outside_after_it_was_unticked_is_ticked_again() {
    use guild_desktop::changes::{Section, to_tick};
    let before = [row_of(Section::Changes, "src/lib.rs", false, false)];
    let now = [row_of(Section::Changes, "src/lib.rs", true, false)];
    assert_eq!(to_tick(&before, &now), [PathBuf::from("src/lib.rs")]);
}

/// A conflict resolved with `git add` is a change a commit can take now,
/// and the person who resolved it meant it to be taken.
#[test]
fn a_conflict_resolved_with_git_add_is_ticked() {
    use guild_desktop::changes::{Section, to_tick};
    let before = [row_of(Section::Changes, "src/merge.rs", true, true)];
    let now = [row_of(Section::Changes, "src/merge.rs", true, false)];
    assert_eq!(to_tick(&before, &now), [PathBuf::from("src/merge.rs")]);
}

/// A row whose index side did not move keeps whatever tick the person
/// left on it — an unticked change stays unticked through every map that
/// lands, staged or not.
#[test]
fn a_row_nobody_staged_keeps_the_tick_the_person_left() {
    use guild_desktop::changes::{Section, to_tick};
    let before = [
        row_of(Section::Changes, "src/unstaged.rs", false, false),
        row_of(Section::Changes, "src/staged.rs", true, false),
        row_of(Section::Unversioned, "scratch.txt", false, false),
    ];
    let now = before.clone();
    assert!(to_tick(&before, &now).is_empty());
    let unstaged_by_hand = [row_of(Section::Changes, "src/staged.rs", false, false)];
    assert!(
        to_tick(&before, &unstaged_by_hand).is_empty(),
        "unstaging is not a reason to tick"
    );
}

/// A conflict is never ticked, whether it arrives from nowhere or from a
/// change that was listed: a commit cannot take it, and `git add` on it
/// would call it resolved.
#[test]
fn a_conflict_is_never_ticked() {
    use guild_desktop::changes::{Section, to_tick};
    let now = [row_of(Section::Changes, "src/merge.rs", true, true)];
    assert!(to_tick(&[], &now).is_empty());
    let before = [row_of(Section::Changes, "src/merge.rs", false, false)];
    assert!(to_tick(&before, &now).is_empty());
    let arrived = [row_of(Section::Changes, "src/new.rs", false, false)];
    assert_eq!(
        to_tick(&[], &arrived),
        [PathBuf::from("src/new.rs")],
        "and a change from nowhere still arrives ticked, as it always has"
    );
}

/// **A pin, not a failing test.** `commit_plan` already leaves a ticked
/// staged row alone; this holds the other half of decision 16's promise —
/// that what `to_tick` ticks is then taken by the commit rather than
/// unstaged by it.
#[test]
fn a_ticked_staged_row_is_not_unstaged_by_the_commit() {
    use guild_desktop::changes::commit_plan;
    use guild_desktop::git::status::{Entry, Kind};
    let map = StatusMap::from_entries(vec![Entry {
        path: PathBuf::from("notes.md"),
        kind: Kind::Added,
        staged: true,
        unstaged: false,
        is_dir: false,
    }]);
    let ticked: HashSet<PathBuf> = [PathBuf::from("notes.md")].into_iter().collect();
    let plan = commit_plan(&map, &ticked, &HashSet::new());
    assert!(plan.unstage.is_empty(), "{plan:?}");
    assert!(plan.stage.is_empty(), "already staged: {plan:?}");
}

#[test]
fn discovered_commit_targets_stage_exact_files_without_empty_or_ignored_content() {
    use guild_desktop::{
        changes::{commit_plan, discover_directory, eligible_targets, list},
        git::{self, Write},
    };
    let fixture = tempfile::tempdir().unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(fixture.path())
            .status()
            .unwrap()
            .success()
    );
    std::fs::create_dir_all(fixture.path().join("new/empty")).unwrap();
    std::fs::write(fixture.path().join(".gitignore"), "*.ignored\n").unwrap();
    for path in ["new/one.txt", "new/two.txt", "new/secret.ignored"] {
        std::fs::write(fixture.path().join(path), "fixture").unwrap();
    }
    let snapshot = discover_directory(fixture.path(), Path::new("new")).unwrap();
    assert!(snapshot.empty_directories.contains(Path::new("new/empty")));
    let tree = guild_desktop::changes::discover_directory_tree(fixture.path(), Path::new("new"));
    assert!(
        tree.get(Path::new("new/empty"))
            .unwrap()
            .as_ref()
            .unwrap()
            .empty
    );
    assert_eq!(
        tree.get(Path::new("new"))
            .unwrap()
            .as_ref()
            .unwrap()
            .files
            .len(),
        2
    );

    let map = StatusMap::from_entries(parse(b"? new/\0").unwrap());
    let directories = std::collections::HashMap::from([("new/".into(), Ok(snapshot))]);
    assert_eq!(
        eligible_targets(&list(&map), &directories, &["new/".into()])
            .unwrap()
            .len(),
        2
    );
    let ticks = HashSet::from([PathBuf::from("new/one.txt")]);
    git::write(
        fixture.path(),
        &Write::Stage(commit_plan(&map, &ticks, &HashSet::new()).stage),
    )
    .unwrap();
    let staged = git::run(fixture.path(), &["ls-files", "-z"]).unwrap();
    assert_eq!(staged, b"new/one.txt\0");
    assert!(fixture.path().join("new/empty").is_dir());
    assert!(!fixture.path().join("new/empty/.gitkeep").exists());
}

#[test]
fn discovered_literal_names_never_stage_excluded_matching_siblings() {
    use guild_desktop::{
        changes::{commit_plan, discover_directory},
        git::{self, Write},
    };
    for literal in ["new/*.txt", "new/[ab].txt", ":(glob)*.txt"] {
        let fixture = tempfile::tempdir().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(fixture.path())
                .status()
                .unwrap()
                .success()
        );
        std::fs::create_dir(fixture.path().join("new")).unwrap();
        for path in [literal, "new/other.txt", "new/a.txt", "other.txt"] {
            std::fs::write(fixture.path().join(path), "data").unwrap();
        }
        let snapshot = discover_directory(fixture.path(), Path::new(".")).unwrap();
        assert!(snapshot.files.contains(&PathBuf::from(literal)));
        let map = StatusMap::from_entries(parse(b"? new/\0").unwrap());
        let paths = if literal.starts_with("new/") {
            commit_plan(&map, &HashSet::from([literal.into()]), &HashSet::new()).stage
        } else {
            vec![literal.into()]
        };
        git::write(fixture.path(), &Write::Stage(paths)).unwrap();
        let expected = format!("{literal}\0");
        assert_eq!(
            git::run(fixture.path(), &["ls-files", "-z"]).unwrap(),
            expected.as_bytes(),
            "literal {literal} must not stage matching siblings"
        );
    }
}

#[test]
fn an_untracked_project_root_discovers_and_stages_explicit_children() {
    use guild_desktop::{
        changes::{commit_plan, discover_directory_tree, eligible_targets, list},
        git::{self, Write},
    };
    let repo = tempfile::tempdir().unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success()
    );
    let root = repo.path().join("project-name");
    std::fs::create_dir_all(root.join("nested/deeper")).unwrap();
    std::fs::write(root.join("one.txt"), "one").unwrap();
    std::fs::write(root.join("nested/deeper/two.txt"), "two").unwrap();
    let entries = parse(&git::status(&root).unwrap()).unwrap();
    let (inside, _) = guild_desktop::git::status::within(entries, Path::new("project-name"));
    let map = StatusMap::from_entries(inside);
    assert!(
        list(&map)
            .iter()
            .any(|l| l.row.path.as_os_str().is_empty() && l.row.is_dir)
    );
    let tree = discover_directory_tree(&root, Path::new(""));
    let targets = eligible_targets(&list(&map), &tree, &[PathBuf::new()]).unwrap();
    assert_eq!(
        targets,
        vec![
            PathBuf::from("nested/deeper/two.txt"),
            PathBuf::from("one.txt")
        ]
    );
    let plan = commit_plan(&map, &targets.into_iter().collect(), &HashSet::new());
    git::write(&root, &Write::Stage(plan.stage)).unwrap();
    assert_eq!(
        git::run(&root, &["ls-files", "-z"]).unwrap(),
        b"nested/deeper/two.txt\0one.txt\0"
    );
}

#[test]
fn unsupported_discovered_identities_cannot_enable_inclusion() {
    use guild_desktop::changes::{Directory, EligibilityError, eligible_targets, list};
    use std::os::unix::ffi::OsStringExt;
    let path = PathBuf::from(std::ffi::OsString::from_vec(b"new/\xff.txt".to_vec()));
    let map = StatusMap::from_entries(parse(b"? new/\0").unwrap());
    let directories = std::collections::HashMap::from([(
        "new/".into(),
        Ok(Directory {
            files: vec![path],
            ..Default::default()
        }),
    )]);
    assert_eq!(
        eligible_targets(&list(&map), &directories, &["new/".into()]),
        Err(EligibilityError::UnsupportedPath)
    );
}

#[test]
fn embedded_nul_discovered_identity_cannot_enable_inclusion() {
    use guild_desktop::changes::{Directory, eligible_targets, list};
    let map = StatusMap::from_entries(parse(b"? new/\0").unwrap());
    let dirs = std::collections::HashMap::from([(
        "new/".into(),
        Ok(Directory {
            files: vec!["new/a.txt\0new/b.txt".into()],
            ..Default::default()
        }),
    )]);
    let error = eligible_targets(&list(&map), &dirs, &["new/".into()]).unwrap_err();
    assert!(error.to_string().contains("NUL") && !error.to_string().contains("UTF-8"));
}

/// A map of tracked modified files, each `(path, staged, unstaged)`.
fn modified(rows: &[(&str, bool, bool)]) -> StatusMap {
    let zero = "0".repeat(40);
    let mut porcelain = Vec::new();
    for (path, staged, unstaged) in rows {
        let x = if *staged { 'M' } else { '.' };
        let y = if *unstaged { 'M' } else { '.' };
        porcelain.extend(
            format!("1 {x}{y} N... 100644 100644 100644 {zero} {zero} {path}\0").into_bytes(),
        );
    }
    StatusMap::from_entries(parse(&porcelain).expect("porcelain v2"))
}

fn set(paths: &[&str]) -> HashSet<PathBuf> {
    paths.iter().map(PathBuf::from).collect()
}

/// GLD-026: a row staged and unstaged at once is partial, and its index is
/// committed as it is — neither added whole nor taken back out.
#[test]
fn a_partial_row_is_committed_as_its_index_stands() {
    use guild_desktop::changes::commit_plan;
    let map = modified(&[
        ("a.rs", true, true),
        ("b.rs", true, true),
        ("c.rs", false, true),
    ]);
    let plan = commit_plan(&map, &set(&["c.rs"]), &set(&["a.rs"]));
    assert_eq!(plan.stage, [PathBuf::from("c.rs")]);
    assert_eq!(
        plan.unstage,
        [PathBuf::from("b.rs")],
        "a cleared row is still taken out; the partial one is left alone"
    );
}

/// Review r3 `bugs+impl-1`: **a status transition alone never adds a path
/// to the commit**. An excluded row stays excluded whichever way its sides
/// turn, a full one stays full (the person's choice: the commit takes the
/// file whole, as before GLD-026), and only a partial tick follows git —
/// full once nothing of the row is unstaged, which commits the same bytes,
/// cleared once nothing of it is staged, gone with its row.
#[test]
fn a_status_transition_never_adds_a_path() {
    use guild_desktop::changes::{commit_plan, retick, sides};
    // Before: excluded-edited M., excluded-matched MM, full-edited M., and
    // the three partial rows MM.
    let map = modified(&[
        ("excluded-edited.rs", true, true),
        ("excluded-matched.rs", true, false),
        ("full-edited.rs", true, true),
        ("done.rs", true, false),
        ("undone.rs", false, true),
        ("kept.rs", true, true),
        ("other.rs", false, true),
    ]);
    let now = sides(&map);
    let mut ticked = set(&["full-edited.rs", "other.rs"]);
    let mut partial = set(&["done.rs", "undone.rs", "kept.rs", "gone.rs"]);
    retick(&now, &mut ticked, &mut partial);
    assert_eq!(ticked, set(&["full-edited.rs", "done.rs", "other.rs"]));
    assert_eq!(partial, set(&["kept.rs"]));
    // Committing another file takes both excluded rows' staged bytes out.
    let plan = commit_plan(&map, &ticked, &partial);
    assert!(
        ["excluded-edited.rs", "excluded-matched.rs"]
            .iter()
            .all(|path| plan.unstage.contains(&PathBuf::from(path))),
        "{plan:?}"
    );
}

#[test]
fn a_partial_row_alone_is_enough_to_commit() {
    use guild_desktop::changes::can_commit;
    assert!(can_commit(&HashSet::new(), &set(&["a.rs"]), "Fix"));
    assert!(!can_commit(&HashSet::new(), &HashSet::new(), "Fix"));
}

/// The person's diff review of r5: in a partial clone the settling walk
/// detects no renames, so a staged rename `old → new` comes back as `D old`
/// and `A new`. `old` was never a row's path — it was the rename's `from` —
/// and it takes the rename row's tick, not a new one: an excluded rename
/// stays out, an included one stays in.
#[test]
fn a_split_rename_source_takes_the_rename_rows_tick() {
    use guild_desktop::changes::{list, ticks_for};
    let zero = "0".repeat(40);
    let renamed = StatusMap::from_entries(
        parse(format!("2 R. N... 100644 100644 100644 {zero} {zero} R100 new\0old\0").as_bytes())
            .unwrap(),
    );
    let split = StatusMap::from_entries(
        parse(
            format!(
                "1 D. N... 100644 000000 000000 {zero} {zero} old\0\
                 1 A. N... 000000 100644 100644 {zero} {zero} new\0"
            )
            .as_bytes(),
        )
        .unwrap(),
    );
    let (before, now) = (list(&renamed), list(&split));
    assert!(
        ticks_for(&before, &now, &set(&[])).is_empty(),
        "excluded stays out"
    );
    assert_eq!(
        ticks_for(&before, &now, &set(&["new"])),
        vec![PathBuf::from("old")]
    );
}
