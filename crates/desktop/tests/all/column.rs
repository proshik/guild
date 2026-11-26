//! The column as a model — M12.
//!
//! Pure: which groups there are, which rows each holds and in what order,
//! what a row is called, where a new worktree's folder goes, and when one
//! may be removed. The lists are the ones M12's task 0 recorded from a
//! real git.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_desktop::column::{
    EDGE, Group, NARROWEST, Opened, Removal, Removing, Row, Tile, Title, WIDEST, folder_for,
    group_key, groups, name_groups, narrowest, opened_in, peek, refuse_removal, refused_removal,
    removal, removed_outside, row_key, tile_colour, title, width,
};
use guild_desktop::git::worktrees::parse;
use guild_session::{ProjectId, WorkspaceId};

/// A main worktree, a detached one, a linked one, a prunable one, a
/// locked one and one under a path with a space.
const SIX: &[u8] = include_bytes!("../fixtures/worktrees-list.bin");

fn common() -> PathBuf {
    PathBuf::from("/Users/person/work/repo/.git")
}

fn opened<'a>(project: ProjectId, path: &'a Path, repository: Option<&'a Path>) -> Opened<'a> {
    Opened {
        project,
        path,
        name: path.file_name().unwrap().to_str().unwrap(),
        repository,
    }
}

/// Two real directories, `repo` and `repo-feat`, for the tests that ask
/// the model whether a root git has not listed still has its folder — the
/// one question it asks of the disk. A test's roots outside git can name
/// any path and the model calls it whatever it is called; a root the model
/// must either draw or drop needs a folder that exists, or one that does
/// not.
struct Live {
    common: PathBuf,
    main: PathBuf,
    feat: PathBuf,
}

/// In `base`, which the test holds: a helper that made the folder would
/// drop it on the way out.
fn live_repo(base: &Path) -> Live {
    let main = base.join("repo");
    let feat = base.join("repo-feat");
    std::fs::create_dir_all(&main).unwrap();
    std::fs::create_dir_all(&feat).unwrap();
    Live {
        common: base.join("repo/.git"),
        main,
        feat,
    }
}

fn named(text: &str) -> Title {
    Title {
        name: text.to_owned(),
        primary: false,
    }
}

fn starred(text: &str) -> Title {
    Title {
        name: text.to_owned(),
        primary: true,
    }
}

#[test]
fn a_repository_is_one_group_its_primary_first_and_git_s_order_after() {
    let (main, feat) = (ProjectId::new(), ProjectId::new());
    let common = common();
    let main_path = PathBuf::from("/Users/person/work/repo");
    let feat_path = PathBuf::from("/Users/person/work/repo-feat");
    let roots = [
        opened(feat, &feat_path, Some(&common)),
        opened(main, &main_path, Some(&common)),
    ];
    let lists = HashMap::from([(common.clone(), parse(SIX).unwrap())]);

    let found = groups(&roots, &lists);

    assert_eq!(found.len(), 1, "one repository, one group");
    let group = &found[0];
    assert_eq!(group.name, "repo");
    assert!(!group.plain);
    assert_eq!(
        group.rows,
        [
            Row::Opened {
                project: main,
                title: starred("main"),
                branch: Some("main".to_owned()),
                committed: None,
                lines: None,
            },
            Row::Unopened {
                path: PathBuf::from("/Users/person/work/repo-detached"),
                title: named("detached"),
                branch: None,
                committed: None,
                lines: None,
            },
            Row::Opened {
                project: feat,
                title: named("feat"),
                branch: Some("feat".to_owned()),
                committed: None,
                lines: None,
            },
            // The prunable one has no folder, and is not drawn.
            Row::Unopened {
                path: PathBuf::from("/Users/person/work/repo-locked"),
                title: named("locked"),
                branch: Some("locked".to_owned()),
                committed: None,
                lines: None,
            },
            Row::Unopened {
                path: PathBuf::from("/Users/person/work/with space/wt"),
                title: named("spaced"),
                branch: Some("spaced".to_owned()),
                committed: None,
                lines: None,
            },
        ]
    );
}

#[test]
fn a_root_git_has_not_listed_yet_still_has_its_row() {
    // At start the list has not landed; the column is drawn from what the
    // workspace remembers and git confirms it a moment later.
    let (main, feat) = (ProjectId::new(), ProjectId::new());
    let base = Scratch::new("guild-column-unlisted-");
    let live = live_repo(&base);
    let common = live.common;
    let main_path = live.main;
    let feat_path = live.feat;
    let roots = [
        opened(feat, &feat_path, Some(&common)),
        opened(main, &main_path, Some(&common)),
    ];

    let found = groups(&roots, &HashMap::new());

    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].rows,
        [
            Row::Opened {
                project: main,
                title: starred("repo"),
                branch: None,
                committed: None,
                lines: None,
            },
            Row::Opened {
                project: feat,
                title: named("feat"),
                branch: None,
                committed: None,
                lines: None,
            },
        ],
        "the primary is first even before git has said so: it is the one beside the .git"
    );
}

/// A disconnected volume must not make an open seat's buffers unreachable.
#[test]
fn a_root_whose_folder_is_gone_remains_reachable() {
    let (main, gone) = (ProjectId::new(), ProjectId::new());
    let base = Scratch::new("guild-column-gone-");
    let live = live_repo(&base);
    let gone_path = live.main.parent().unwrap().join("gone-worktree");
    let _ = std::fs::remove_dir_all(&gone_path); // it must not exist
    let roots = [
        opened(main, &live.main, Some(&live.common)),
        opened(gone, &gone_path, Some(&live.common)),
    ];

    let found = groups(&roots, &HashMap::new());

    let opened: Vec<ProjectId> = found[0]
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Opened { project, .. } => Some(*project),
            _ => None,
        })
        .collect();
    assert_eq!(
        opened,
        vec![main, gone],
        "the unavailable root stays reachable until explicitly closed"
    );
}

/// **A worktree removed outside the window is closed by the window**: a tool
/// outside Guild removing worktrees left the column drawing every one of
/// them, because the roots were open. Git's list is the witness, and a folder
/// with no `.git` in it is the second one: a disconnected volume gives no
/// list, and a checkout git has not listed yet still has its `.git`. Unsaved
/// text or an agent at work keeps the row, because closing would decide for
/// the person.
#[test]
fn a_root_removed_outside_the_window_is_closed_unless_it_holds_something() {
    let base = Scratch::new("guild-column-removed-");
    let live = live_repo(&base);
    // The main checkout carries `.git`; `repo-feat` stands for a folder
    // whose checkout went and whose `.guild/` stayed behind.
    std::fs::create_dir_all(&live.common).unwrap();
    let _ = std::fs::remove_file(live.feat.join(".git"));
    let list = parse(
        format!(
            "worktree {}\0HEAD 467eaac8b04426ce7ffd85ced21a2188c38414c6\0branch refs/heads/master\0\0",
            live.main.display()
        )
        .as_bytes(),
    )
    .unwrap();
    let gone = live.feat.parent().unwrap().join("repo-deleted");

    // unsaved, live
    assert!(removed_outside(&list, &live.feat, false, false));
    assert!(removed_outside(&list, &gone, false, false));
    assert!(
        !removed_outside(&list, &live.feat, true, false),
        "unsaved text keeps it"
    );
    assert!(
        !removed_outside(&list, &live.feat, false, true),
        "a live agent keeps it"
    );
    assert!(
        !removed_outside(&list, &live.main, false, false),
        "a root git lists is never removed"
    );

    // A checkout git has not listed yet — made a moment ago, the list a
    // tick behind — still has its `.git`.
    std::fs::write(live.feat.join(".git"), "gitdir: somewhere\n").unwrap();
    assert!(!removed_outside(&list, &live.feat, false, false));
    std::fs::remove_file(live.feat.join(".git")).unwrap();

    // Listed but prunable is git saying the folder is gone.
    let prunable = parse(
        format!(
            "worktree {}\0HEAD 467eaac8b04426ce7ffd85ced21a2188c38414c6\0branch refs/heads/feat\0prunable gitdir file points to non-existent location\0\0",
            live.feat.display()
        )
        .as_bytes(),
    )
    .unwrap();
    assert!(removed_outside(&prunable, &live.feat, false, false));
}

#[test]
fn a_directory_outside_git_is_a_group_of_one() {
    let plain = ProjectId::new();
    let path = PathBuf::from("/Users/person/notes");
    let found = groups(&[opened(plain, &path, None)], &HashMap::new());

    assert_eq!(
        found,
        [Group {
            name: "notes".to_owned(),
            key: path.clone(),
            repository: None,
            plain: true,
            rows: vec![Row::Opened {
                project: plain,
                title: named("notes"),
                branch: None,
                committed: None,
                lines: None,
            }],
        }]
    );
}

#[test]
fn two_repositories_are_two_groups_in_the_order_they_were_opened() {
    let (a, b, c) = (ProjectId::new(), ProjectId::new(), ProjectId::new());
    let base = Scratch::new("guild-column-two-");
    for sub in ["meadow", "guild", "guild-lazy-lsp"] {
        std::fs::create_dir_all(base.join(sub)).unwrap();
    }
    let (guild, meadow) = (base.join("guild/.git"), base.join("meadow/.git"));
    let (pa, pb, pc) = (
        base.join("meadow"),
        base.join("guild"),
        base.join("guild-lazy-lsp"),
    );
    let roots = [
        opened(a, &pa, Some(&meadow)),
        opened(b, &pb, Some(&guild)),
        opened(c, &pc, Some(&guild)),
    ];

    let found = groups(&roots, &HashMap::new());

    let names: Vec<&str> = found.iter().map(|group| group.name.as_str()).collect();
    assert_eq!(names, ["meadow", "guild"]);
    assert_eq!(found[1].rows.len(), 2);
}

/// **A worktree is called by its branch, and the main one carries a star**
/// — M12.2 task 2. M12 called the main one
/// *primary*, a word nobody had asked for, and every other one by its
/// folder with the branch under it. A worktree with no branch — detached —
/// is still called by its folder, without the repository's prefix Guild
/// gives the ones it makes.
#[test]
fn a_row_is_called_by_its_branch_and_the_main_one_is_starred() {
    // On a branch: the branch, whatever the folder is called.
    assert_eq!(
        title(
            "guild",
            Path::new("/w/guild-lazy-lsp"),
            false,
            Some("lazy-lsp")
        ),
        named("lazy-lsp")
    );
    assert_eq!(
        title(
            "guild",
            Path::new("/w/elsewhere/wt"),
            false,
            Some("feature/login")
        ),
        named("feature/login")
    );
    // The main one: its branch, starred.
    assert_eq!(
        title("guild", Path::new("/w/guild"), true, Some("main")),
        starred("main")
    );
    // Detached: the folder, without Guild's prefix.
    assert_eq!(
        title("guild", Path::new("/w/guild-lazy-lsp"), false, None),
        named("lazy-lsp")
    );
    assert_eq!(
        title("guild", Path::new("/w/elsewhere/wt"), false, None),
        named("wt")
    );
    assert_eq!(
        title("guild", Path::new("/w/guild"), true, None),
        starred("guild")
    );
    // A prefix with nothing after it is not a name.
    assert_eq!(
        title("guild", Path::new("/w/guild-"), false, None),
        named("guild-")
    );
}

#[test]
fn a_new_worktree_goes_beside_the_repository_named_after_it() {
    assert_eq!(
        folder_for(Path::new("/w/guild"), "lazy-lsp"),
        Path::new("/w/guild-lazy-lsp")
    );
    // A slash stays in the branch and becomes a dash in the folder.
    assert_eq!(
        folder_for(Path::new("/w/guild"), "feat/column"),
        Path::new("/w/guild-feat-column")
    );
}

/// The order is what a person is told first, and it is not git's order.
///
/// **The lock arrived in M12 task 18, from the window**: git refuses a
/// locked worktree before it looks at anything else, and without this the
/// question was asked, answered, and then met with git's own *cannot
/// remove a locked working tree* — six lines of terminal English in a
/// notice. The list already says which are locked and why.
#[test]
fn removal_refuses_what_would_lose_work_and_asks_otherwise() {
    // primary, agent busy, locked, changed
    assert_eq!(removal(true, false, false, false), Removal::Primary);
    assert_eq!(removal(false, true, false, false), Removal::AgentBusy);
    assert_eq!(removal(false, false, true, false), Removal::Locked);
    assert_eq!(removal(false, false, false, true), Removal::Changed);

    // An agent at work is the first thing to say, before either of git's
    // two refusals: losing its turn is the thing that cannot be undone.
    assert_eq!(removal(false, true, true, true), Removal::AgentBusy);
    // And a lock is said before changes, because it is the one git checks
    // first — a person who unlocks would only then meet the other.
    assert_eq!(removal(false, false, true, true), Removal::Locked);

    assert_eq!(removal(false, false, false, false), Removal::Ask);
}

/// **What `guild worktree remove` refuses, in what order, and in words an
/// agent can act on** — M13 task 8. Each case is the one fact that stops it,
/// and each sentence names the folder and what would have to change.
#[test]
fn a_command_s_removal_is_refused_by_the_first_thing_that_would_lose_something() {
    let path = Path::new("/w/repo-feat");
    let clear = Removing {
        path,
        caller_is_task: false,
        primary: false,
        agent_busy: false,
        persons_session: false,
        unsaved: false,
        lock: None,
        changed: false,
        ignored: &[],
        delete_ignored: false,
    };
    assert_eq!(refuse_removal(&clear), None, "nothing stands in the way");

    let said = |asked: Removing| refuse_removal(&asked).expect("refused");

    let task = said(Removing {
        caller_is_task: true,
        primary: true,
        ..clear
    });
    assert!(task.contains("task"), "who asks is said first: {task}");

    assert!(
        said(Removing {
            primary: true,
            agent_busy: true,
            ..clear
        })
        .contains("own checkout")
    );
    let busy = said(Removing {
        agent_busy: true,
        persons_session: true,
        lock: Some("x"),
        ..clear
    });
    assert!(busy.contains("agent"), "{busy}");
    let person = said(Removing {
        persons_session: true,
        unsaved: true,
        ..clear
    });
    assert!(person.contains("person opened"), "{person}");
    let unsaved = said(Removing {
        unsaved: true,
        lock: Some("x"),
        ..clear
    });
    assert!(unsaved.contains("unsaved"), "{unsaved}");

    // git's own reason, whole, because whoever locked it knew what they held.
    let locked = said(Removing {
        lock: Some("claude session (pid 35060)"),
        changed: true,
        ..clear
    });
    assert!(locked.contains("claude session (pid 35060)"), "{locked}");
    let changed = said(Removing {
        changed: true,
        ignored: &["target/".to_owned()],
        ..clear
    });
    assert!(changed.contains("changes"), "{changed}");
    assert!(
        changed.contains("git status --short --untracked-files=all"),
        "a refusal must explain how to find the blocking paths: {changed}"
    );
    assert!(changed.contains("tracked or untracked"), "{changed}");
    assert_eq!(
        said(Removing {
            changed: true,
            ignored: &["target/".to_owned()],
            delete_ignored: true,
            ..clear
        }),
        changed,
        "ignored-file consent cannot bypass actual changes"
    );

    // The ignored paths are named, one to a line, and the flag that lets
    // them go is named with them.
    let ignored = ["target/".to_owned(), ".env".to_owned()];
    let named = said(Removing {
        ignored: &ignored,
        ..clear
    });
    assert!(named.contains("\n  target/\n  .env"), "{named}");
    assert!(named.contains("--delete-ignored"), "{named}");
    assert_eq!(
        refuse_removal(&Removing {
            ignored: &ignored,
            delete_ignored: true,
            ..clear
        }),
        None,
        "consent given"
    );

    for sentence in [task, busy, person, unsaved, locked, changed, named] {
        assert!(
            sentence.contains("/w/repo-feat") || sentence.contains("task"),
            "{sentence}"
        );
    }
}

/// **A long list of ignored paths is cut, and says how much was cut** — a
/// `node_modules` walked by hand would otherwise be a refusal nobody reads
/// to the end.
#[test]
fn a_long_list_of_ignored_paths_says_how_many_more() {
    let ignored: Vec<String> = (0..30).map(|n| format!("build-{n}/")).collect();
    let said = refuse_removal(&Removing {
        path: Path::new("/w/repo-feat"),
        caller_is_task: false,
        primary: false,
        agent_busy: false,
        persons_session: false,
        unsaved: false,
        lock: None,
        changed: false,
        ignored: &ignored,
        delete_ignored: false,
    })
    .unwrap();
    assert!(said.contains("build-19/"), "{said}");
    assert!(!said.contains("build-20/"), "{said}");
    assert!(said.contains("and 10 more"), "{said}");
}

/// **git's refusal keeps git's words, and does not keep git's advice.**
/// `worktree remove` refuses a changed checkout with *use --force to delete
/// it*, and an agent reads advice as an instruction; the window never
/// forces, so its answer says what to do instead — M13 task 8.
#[test]
fn git_s_refusal_to_remove_says_what_to_do_instead_of_forcing() {
    let git =
        "fatal: '/w/repo-feat' contains modified or untracked files, use --force to delete it";
    let said = refused_removal(git);
    assert!(said.starts_with(git), "{said}");
    assert!(said.contains("never forces"), "{said}");
    assert!(said.contains("ask the person"), "{said}");
}

/// **A group is called what the person named it, by its key** — M12.2
/// task 9. A repository is known by its common git directory, so all its
/// worktrees' groups agree; a directory outside git by itself. A group
/// nobody named keeps its folder's name.
#[test]
fn a_group_is_called_what_the_person_named_it() {
    let common = common();
    let (main, plain) = (ProjectId::new(), ProjectId::new());
    let main_path = PathBuf::from("/Users/person/work/repo");
    let notes = PathBuf::from("/Users/person/notes");
    let roots = [
        opened(main, &main_path, Some(&common)),
        opened(plain, &notes, None),
    ];
    let mut found = groups(&roots, &HashMap::new());
    assert_eq!(found[0].key, common, "a repository by its common directory");
    assert_eq!(found[1].key, notes, "a directory by itself");

    name_groups(&mut found, |key| {
        (key == common).then(|| "Work repo".to_owned())
    });
    assert_eq!(found[0].name, "Work repo");
    assert_eq!(found[1].name, "notes", "nobody named it");
}

/// **The defect the window found, pinned.** Every repository's main
/// worktree is called by the same word of the interface, so two of them
/// open at once drew two rows called `primary` — and a key made from that
/// name is one key for two rows. GPUI keeps a stateful element's state by
/// its id, so the second row's press was recorded against the first and
/// clicking it never changed the project: the column looked as though only
/// the first repository could be opened.
///
/// A key is made from what is unique by construction, and this is the test
/// that was missing when the gestures could not be driven from outside.
#[test]
fn two_repositories_primary_rows_have_keys_of_their_own() {
    let opened = |project| Row::Opened {
        project,
        title: starred("main"),
        branch: Some("main".to_owned()),
        committed: None,
        lines: None,
    };
    let unopened = |path: &str| Row::Unopened {
        path: PathBuf::from(path),
        title: starred("repo"),
        branch: None,
        committed: None,
        lines: None,
    };

    let one = ProjectId::new();
    let two = ProjectId::new();
    let workspace = WorkspaceId::new();
    assert_ne!(
        row_key(workspace, &opened(one)),
        row_key(workspace, &opened(two))
    );
    assert_eq!(
        row_key(workspace, &opened(one)),
        row_key(workspace, &opened(one))
    );

    // The same for two nobody has opened: they are told apart by folder.
    assert_ne!(
        row_key(workspace, &unopened("/a/repo")),
        row_key(workspace, &unopened("/b/repo"))
    );

    // And the two kinds never collide with each other.
    assert_ne!(
        row_key(workspace, &opened(one)),
        row_key(workspace, &unopened("/a/repo"))
    );

    // The same root in two workspaces is two stateful rows, because its
    // selection, expansion and tabs belong to the seat rather than the root.
    assert_ne!(
        row_key(workspace, &opened(one)),
        row_key(WorkspaceId::new(), &opened(one))
    );
}

/// The column that is hidden comes back by the edge and goes by its own
/// width — M12 task 10, and the asymmetry is the point.
#[test]
fn a_hidden_column_comes_out_at_the_edge_and_goes_when_the_pointer_leaves_it() {
    const WIDTH: f32 = 232.0;

    // Not folded: it is already there, and nothing peeks.
    assert!(!peek(false, false, 1.0, WIDTH));
    assert!(!peek(false, true, 1.0, WIDTH));

    // Folded and away: only the edge brings it out.
    assert!(peek(true, false, 0.0, WIDTH));
    assert!(peek(true, false, EDGE, WIDTH));
    assert!(!peek(true, false, EDGE + 0.5, WIDTH));
    assert!(!peek(true, false, 100.0, WIDTH));

    // Folded and out: it stays for the whole of its own width, so a
    // person crossing it to its button does not lose it under the
    // pointer, and goes the moment the pointer is past it.
    assert!(peek(true, true, EDGE + 0.5, WIDTH));
    assert!(peek(true, true, WIDTH, WIDTH));
    assert!(!peek(true, true, WIDTH + 0.5, WIDTH));
}

/// The border is dragged to where the pointer is, and stops at both ends
/// — M12 task 11. The column is against the window's left edge, so the
/// pointer's x is the width itself.
#[test]
fn the_column_is_as_wide_as_the_pointer_between_its_two_ends() {
    let font = guild_session::Settings::default().ui_font_size;
    assert_eq!(width(232.0, font), 232.0);
    assert_eq!(width(300.0, font), 300.0);

    // Narrower than the floor and the title bar's controls over the column
    // spill past its edge; wider than the ceiling and the panel is taking
    // room from the work it exists to reach.
    assert_eq!(width(0.0, font), narrowest(font));
    assert_eq!(width(narrowest(font) - 40.0, font), narrowest(font));
    assert_eq!(width(10_000.0, font), WIDEST);
}

/// The column's floor holds what the title bar draws over it — the
/// traffic lights, the column and history buttons and the mode button at
/// its edge — at every interface size, and never drops below the rows'
/// own floor.
#[test]
fn the_floor_grows_with_the_interface_and_holds_the_title_bar() {
    let sizes = [
        guild_session::MIN_UI_FONT_SIZE,
        13.0,
        16.0,
        guild_session::MAX_UI_FONT_SIZE,
    ];
    let floors: Vec<f32> = sizes.iter().map(|font| narrowest(*font)).collect();
    assert!(floors.iter().all(|floor| *floor >= NARROWEST));
    assert!(floors.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(
        narrowest(13.0) > 180.0,
        "the old 180 left the mode button outside"
    );
    assert!(narrowest(guild_session::MAX_UI_FONT_SIZE) <= WIDEST);
}

/// A settings file can hold an interface size past the settings' range; the
/// floor never passes the widest column, so a width is still a width.
#[test]
fn a_saved_interface_size_past_the_range_keeps_the_floor_under_the_widest() {
    for font in [30.0, 64.0] {
        assert!(narrowest(font) <= WIDEST);
        assert_eq!(width(100.0, font), narrowest(font));
        assert_eq!(width(1000.0, font), WIDEST);
    }
}

/// **A row carries the lines git counted for its worktree** — M12.4, the
/// `+677 −298` — and a root git has not listed yet
/// carries none, because nobody has counted anything for it.
#[test]
fn a_row_carries_the_lines_git_counted_for_its_worktree() {
    let (main, feat) = (ProjectId::new(), ProjectId::new());
    let common = common();
    let main_path = PathBuf::from("/Users/person/work/repo");
    let feat_path = PathBuf::from("/Users/person/work/repo-feat");
    let roots = [
        opened(main, &main_path, Some(&common)),
        opened(feat, &feat_path, Some(&common)),
    ];
    let mut list = parse(SIX).unwrap();
    for worktree in &mut list {
        if worktree.path == feat_path {
            worktree.lines = Some((677, 298));
        }
    }
    let lists = HashMap::from([(common.clone(), list)]);

    let rows = &groups(&roots, &lists)[0].rows;
    let lines_of = |wanted: ProjectId| {
        rows.iter().find_map(|row| match row {
            Row::Opened { project, lines, .. } if *project == wanted => Some(*lines),
            _ => None,
        })
    };
    assert_eq!(lines_of(feat), Some(Some((677, 298))));
    assert_eq!(
        lines_of(main),
        Some(None),
        "git counted nothing for this one"
    );

    // Before git has listed them nobody has counted anything — the same
    // question, asked of roots whose folders exist.
    let base = Scratch::new("guild-column-unlisted-lines-");
    let live = live_repo(&base);
    let live_roots = [
        opened(main, &live.main, Some(&live.common)),
        opened(feat, &live.feat, Some(&live.common)),
    ];
    let unlisted = groups(&live_roots, &HashMap::new());
    assert!(
        unlisted[0]
            .rows
            .iter()
            .all(|row| matches!(row, Row::Opened { lines: None, .. })),
        "before git has listed them, no row has lines"
    );
}

/// **A repository's tile is its own and the same every time** — M12.4.
/// FNV-1a of the name rather than the standard library's hasher, whose
/// output Rust does not promise to keep: a colour that changed with the
/// compiler would be a project that changed colour. Pinned for one name so
/// a change to the hash is seen, and spread over most of the five for a
/// handful of ordinary repository names, so it is not one colour for all.
#[test]
fn a_repository_s_tile_is_its_own_and_the_names_spread_over_the_five() {
    assert_eq!(tile_colour("meadow"), Tile::Brass);
    assert_eq!(tile_colour("guild"), tile_colour("guild"));
    let names = [
        "atlas", "guild", "meadow", "lantern", "tundra", "deploy", "kestrel",
    ];
    let mut tiles: Vec<Tile> = names.iter().map(|name| tile_colour(name)).collect();
    tiles.sort_by_key(|tile| *tile as u8);
    tiles.dedup();
    assert!(tiles.len() >= 4, "seven names on {} tiles", tiles.len());
}

/// **A root finds the project it is folded under** — M12.5 decision 18.
/// Bringing a root forward unfolds its group, and the key it asks by has
/// to be the key the column drew the group with — for a worktree of a
/// repository and for a plain directory alike; and a folded head wears
/// what its opened rows are doing, so it knows which they are.
#[test]
fn a_root_s_fold_is_kept_by_the_key_its_group_is_drawn_with() {
    let (main, feat, plain) = (ProjectId::new(), ProjectId::new(), ProjectId::new());
    let base = Scratch::new("guild-column-fold-");
    let live = live_repo(&base);
    let common = live.common;
    let main_path = live.main;
    let feat_path = live.feat;
    let plain_path = PathBuf::from("/Users/person/notes");
    let roots = [
        opened(main, &main_path, Some(&common)),
        opened(plain, &plain_path, None),
        opened(feat, &feat_path, Some(&common)),
    ];
    let found = groups(&roots, &HashMap::new());

    for root in &roots {
        let group = found
            .iter()
            .find(|group| opened_in(group).any(|project| project == root.project))
            .expect("every root is in a group");
        assert_eq!(group_key(root.path, root.repository), group.key);
    }
    assert_eq!(opened_in(&found[0]).collect::<Vec<_>>(), [main, feat]);
    assert_eq!(opened_in(&found[1]).collect::<Vec<_>>(), [plain]);
}

/// **A root git already lists is known to be its repository's at once** —
/// 2026-09-24. A worktree made by hand or by `guild worktree create`, then
/// opened from its row, stood for a moment as a group of its own: the
/// root's repository was learnt only when its own git thread answered.
/// Its path is in the repository's list already, and that is the answer.
#[test]
fn a_root_git_already_lists_belongs_to_that_repository_at_once() {
    use guild_desktop::column::listed_in;
    let mut lists = HashMap::new();
    lists.insert(common(), parse(SIX).expect("the fixture parses"));

    assert_eq!(
        listed_in(&lists, Path::new("/Users/person/work/repo-feat")),
        Some(common().as_path())
    );
    assert_eq!(
        listed_in(&lists, Path::new("/Users/person/work/with space/wt")),
        Some(common().as_path())
    );
    assert_eq!(
        listed_in(&lists, Path::new("/Users/person/elsewhere")),
        None,
        "a folder no list names is git's to answer"
    );
}

#[test]
fn missing_linked_worktree_primary_identity_matches_menu_rows() {
    use guild_desktop::column::is_primary_checkout;
    let common = Path::new("/repo/.git");
    let roots = [
        opened(ProjectId::new(), Path::new("/repo"), Some(common)),
        opened(ProjectId::new(), Path::new("/repo-linked"), Some(common)),
    ];
    let found = groups(&roots, &HashMap::new());
    for row in &found[0].rows {
        let Row::Opened { project, title, .. } = row else {
            panic!("opened roots")
        };
        let root = roots.iter().find(|root| root.project == *project).unwrap();
        assert_eq!(
            is_primary_checkout(root.path, root.repository),
            title.primary
        );
    }
    assert!(!is_primary_checkout(
        Path::new("/repo-linked"),
        Some(common)
    ));
    assert!(is_primary_checkout(Path::new("/repo"), Some(common)));
    assert!(is_primary_checkout(Path::new("/plain"), None));
    assert!(!is_primary_checkout(
        Path::new("/bare-linked"),
        Some(Path::new("/bare.git"))
    ));
}

#[test]
fn primary_is_first_when_git_lists_only_a_linked_checkout() {
    let common = Path::new("/repo/.git");
    let roots = [opened(ProjectId::new(), Path::new("/repo"), Some(common))];
    let list = parse(b"worktree /linked\0HEAD 467eaac8b04426ce7ffd85ced21a2188c38414c6\0branch refs/heads/feat\0\0").unwrap();
    let found = groups(&roots, &HashMap::from([(common.to_path_buf(), list)]));
    assert!(matches!(&found[0].rows[0], Row::Opened { title, .. } if title.primary));
}

#[test]
fn project_sort_aggregates_linked_worktrees_and_keeps_rows_and_manual_ties() {
    use guild_desktop::column::sort_groups;
    use guild_session::project_sort::{Activity, Mode};
    let ids = [
        ProjectId::new(),
        ProjectId::new(),
        ProjectId::new(),
        ProjectId::new(),
    ];
    let a = Path::new("/a/.git");
    let b = Path::new("/b/.git");
    let roots = [
        opened(ids[3], Path::new("/unknown"), None),
        opened(ids[1], Path::new("/b"), Some(b)),
        opened(ids[0], Path::new("/a"), Some(a)),
        opened(ids[2], Path::new("/a-linked"), Some(a)),
    ];
    let manual = groups(&roots, &HashMap::new());
    let stats = |row: &Row| match row {
        Row::Opened { project, .. } if *project == ids[0] => Activity {
            turns: 2,
            last_activity: Some(5),
            last_user: Some(1),
            last_turn: Some(4),
        },
        Row::Opened { project, .. } if *project == ids[2] => Activity {
            turns: 3,
            last_activity: Some(9),
            last_user: None,
            last_turn: Some(6),
        },
        Row::Opened { project, .. } if *project == ids[1] => Activity {
            turns: 4,
            last_activity: Some(8),
            last_user: Some(2),
            last_turn: Some(7),
        },
        _ => Activity::default(),
    };
    for (mode, expected) in [
        (Mode::Manual, vec![Path::new("/unknown"), b, a]),
        // B's turn started last although A's tools ran later.
        (Mode::RecentActivity, vec![b, a, Path::new("/unknown")]),
        (Mode::MostActivity, vec![a, b, Path::new("/unknown")]),
        (Mode::LastUserMessage, vec![b, a, Path::new("/unknown")]),
    ] {
        let mut found = manual.clone();
        sort_groups(&mut found, mode, stats, |_| None);
        assert_eq!(
            found
                .iter()
                .map(|group| group.key.as_path())
                .collect::<Vec<_>>(),
            expected
        );
        for group in &found {
            assert_eq!(
                group.rows,
                manual.iter().find(|old| old.key == group.key).unwrap().rows
            );
        }
    }
    let mut tied = manual.clone();
    sort_groups(
        &mut tied,
        Mode::RecentActivity,
        |_| Activity::default(),
        |_| None,
    );
    assert_eq!(tied, manual, "unknown ties retain manual order");
    sort_groups(
        &mut tied,
        Mode::LastUserMessage,
        |_| Activity {
            last_user: Some(5),
            ..Activity::default()
        },
        |_| None,
    );
    assert_eq!(tied, manual, "known ties retain manual order");
    sort_groups(
        &mut tied,
        Mode::MostActivity,
        |_| Activity {
            turns: 1,
            ..Activity::default()
        },
        |_| None,
    );
    assert_eq!(tied[0].key, a, "all of A's worktrees contribute");
}

#[test]
fn project_sort_menu_has_one_exclusive_choice_for_every_mode() {
    use guild_desktop::view::menu::{Deed, Entry, column::sort_entries};
    use guild_session::project_sort::Mode;
    for selected in Mode::ALL {
        let entries = sort_entries(selected);
        assert_eq!(entries.len(), 4);
        let mut checked = Vec::new();
        for (entry, mode) in entries.iter().zip(Mode::ALL) {
            let Entry::Choice(choice) = entry else {
                panic!("sorting is an exclusive choice");
            };
            assert!(choice.enabled);
            assert_eq!(choice.deed, Deed::ProjectSort(mode));
            if choice.check == Some(true) {
                checked.push(mode);
            }
        }
        assert_eq!(checked, [selected]);
    }
}

#[test]
fn recent_activity_lifts_the_group_where_a_turn_last_started() {
    use guild_desktop::column::sort_groups;
    use guild_session::project_sort::Mode;
    use guild_session::status::ReportOutcome;
    use guild_session::{Desk, Runs, SessionMode, SessionRecord};
    let mut desk = Desk::default();
    let paths = ["/a", "/b", "/b-linked", "/c", "/d", "/e"];
    let first = desk.open_in(WorkspaceId::nil(), Path::new(paths[0]));
    let workspace = desk.active_workspace();
    let ids: Vec<_> = std::iter::once(first)
        .chain(
            paths[1..]
                .iter()
                .map(|path| desk.open_in(workspace, Path::new(path))),
        )
        .collect();
    let b_repo = Path::new("/b/.git");
    let roots: Vec<_> = ids
        .iter()
        .zip(paths)
        .map(|(id, path)| {
            let repository = path.starts_with("/b").then_some(b_repo);
            opened(*id, Path::new(path), repository)
        })
        .collect();
    let session = |desk: &mut Desk, at: usize| {
        let project = ids[at];
        desk.add_session(SessionRecord::new(
            paths[at].into(),
            (workspace, project),
            Path::new(paths[at]).into(),
            Runs::Claude(SessionMode::Terminal),
        ))
    };
    let a = session(&mut desk, 0);
    let linked = session(&mut desk, 2);
    let c = session(&mut desk, 3);
    let d = session(&mut desk, 4);
    // What the program reported (OSC 7501): a turn began, or its record
    // changed some other way — a tool, a permission, the turn's end.
    let began = ReportOutcome {
        changed: true,
        began: true,
        ended: false,
    };
    let other = |ended| ReportOutcome {
        changed: true,
        began: false,
        ended,
    };
    let shown = |desk: &Desk| {
        let mut found = groups(&roots, &HashMap::new());
        sort_groups(
            &mut found,
            Mode::RecentActivity,
            |row| match row {
                Row::Opened { project, .. } => {
                    let at = ids.iter().position(|id| id == project).unwrap();
                    desk.project_activity(Path::new(paths[at]))
                }
                Row::Unopened { path, .. } => desk.project_activity(path),
            },
            |_| None,
        );
        found
            .iter()
            .map(|group| group.key.to_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(shown(&desk), ["/a", "/b/.git", "/c", "/d", "/e"]);

    desk.observe_reported_activity(a, began, 10);
    desk.observe_reported_activity(c, began, 20);
    assert_eq!(
        shown(&desk),
        ["/c", "/a", "/b/.git", "/d", "/e"],
        "groups without a turn start keep manual order below"
    );

    for (outcome, at) in [
        (other(false), 30),
        (other(false), 31),
        (other(false), 32),
        (other(true), 33),
    ] {
        desk.observe_reported_activity(a, outcome, at);
        assert_eq!(
            shown(&desk),
            ["/c", "/a", "/b/.git", "/d", "/e"],
            "{outcome:?} in a lower project does not move it"
        );
    }
    desk.observe_reported_activity(d, other(false), 34);
    assert_eq!(
        shown(&desk),
        ["/c", "/a", "/b/.git", "/d", "/e"],
        "a change without a turn start does not lift a project"
    );

    desk.observe_reported_activity(a, began, 40);
    assert_eq!(shown(&desk), ["/a", "/c", "/b/.git", "/d", "/e"]);

    desk.observe_reported_activity(linked, began, 50);
    assert_eq!(
        shown(&desk),
        ["/b/.git", "/a", "/c", "/d", "/e"],
        "a linked worktree's turn start lifts its repository group"
    );
}

/// **Recent activity puts the projects with a running agent session first,
/// in the order they became active** — the person's own scenario. A session
/// runs from its first turn until it fails, exits or its tab closes; rest
/// between turns keeps its group in place, and any row of a group counts,
/// its worktrees included. Once no session of a group runs, the group
/// sorts among the others by its latest turn.
#[test]
fn recent_activity_keeps_running_groups_on_top_in_the_order_they_became_active() {
    use guild_desktop::column::{active_groups, sort_groups};
    use guild_session::project_sort::{Active, Activity, Mode, Running};
    use guild_session::status::{Status, Why};
    let workspace = WorkspaceId::new();
    let elsewhere = WorkspaceId::new();
    let repo = Path::new("/guild/.git");
    let paths = ["/guild", "/guild-wt", "/deploy", "/base13", "/quiet", "/d"];
    let ids: Vec<_> = paths.iter().map(|_| ProjectId::new()).collect();
    let roots: Vec<_> = ids
        .iter()
        .zip(paths)
        .map(|(id, path)| {
            opened(
                *id,
                Path::new(path),
                path.starts_with("/guild").then_some(repo),
            )
        })
        .collect();
    let at = |path: &str| paths.iter().position(|item| *item == path).unwrap();
    let mut last_turn: HashMap<&str, u64> = HashMap::from([
        ("/guild", 10),
        ("/deploy", 20),
        ("/base13", 30),
        ("/quiet", 5),
    ]);
    // One agent session per row, by the row's path: its workspace, whether
    // it is alive, and its status. A row absent here has no open tab.
    let mut tabs: HashMap<&'static str, (WorkspaceId, bool, Status)> = HashMap::new();
    let mut running = Running::default();
    let mut active = Active::default();
    let mut shown =
        |last_turn: &HashMap<&str, u64>,
         tabs: &HashMap<&'static str, (WorkspaceId, bool, Status)>| {
            let sessions = running.update(
                tabs.iter()
                    .map(|(path, (_, alive, status))| (*path, *alive, *status)),
            );
            active.update(active_groups(sessions.iter().map(|path| {
                (tabs[path].0, Path::new(*path), roots[at(path)].repository)
            })));
            let mut found = groups(&roots, &HashMap::new());
            sort_groups(
                &mut found,
                Mode::RecentActivity,
                |row| match row {
                    Row::Opened { project, .. } => Activity {
                        last_turn: last_turn
                            .get(paths[ids.iter().position(|id| id == project).unwrap()])
                            .copied(),
                        ..Activity::default()
                    },
                    Row::Unopened { .. } => Activity::default(),
                },
                |group| active.since(&(workspace, group.key.clone())),
            );
            found
                .iter()
                .map(|group| group.key.to_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
    let by_turn = ["/base13", "/deploy", "/guild/.git", "/quiet", "/d"];
    assert_eq!(shown(&last_turn, &tabs), by_turn, "nothing running");

    for path in ["/guild-wt", "/deploy", "/base13"] {
        tabs.insert(path, (workspace, true, Status::Idle));
    }
    assert_eq!(
        shown(&last_turn, &tabs),
        by_turn,
        "agent tabs opened without a turn activate nothing"
    );

    tabs.insert("/guild-wt", (workspace, true, Status::Working));
    assert_eq!(
        shown(&last_turn, &tabs),
        ["/guild/.git", "/base13", "/deploy", "/quiet", "/d"],
        "a worktree's session makes its repository active, above newer turns"
    );
    tabs.insert("/deploy", (workspace, true, Status::Working));
    tabs.insert("/base13", (workspace, true, Status::Working));
    let three = ["/guild/.git", "/deploy", "/base13", "/quiet", "/d"];
    assert_eq!(
        shown(&last_turn, &tabs),
        three,
        "active in one pass: manual order breaks the tie"
    );

    for (state, turn) in [
        (Status::Waiting(Why::Question), 20),
        (Status::Done, 20),
        // The answer seen: rest between turns.
        (Status::Idle, 20),
        (Status::Working, 50),
        (Status::Waiting(Why::Permission), 50),
        (Status::Idle, 50),
    ] {
        tabs.insert("/deploy", (workspace, true, state));
        last_turn.insert("/deploy", turn);
        assert_eq!(
            shown(&last_turn, &tabs),
            three,
            "{state:?} keeps deploy in its place"
        );
    }

    tabs.insert("/deploy", (workspace, false, Status::Idle));
    assert_eq!(
        shown(&last_turn, &tabs),
        ["/guild/.git", "/base13", "/deploy", "/quiet", "/d"],
        "deploy's agent exited: it heads the others by its latest turn"
    );
    tabs.insert("/d", (workspace, true, Status::Working));
    assert_eq!(
        shown(&last_turn, &tabs),
        ["/guild/.git", "/base13", "/d", "/deploy", "/quiet"],
        "a project that became active later goes to the end of the block"
    );

    tabs.insert("/deploy", (elsewhere, true, Status::Working));
    tabs.insert("/base13", (workspace, true, Status::Failed));
    assert_eq!(
        shown(&last_turn, &tabs),
        ["/guild/.git", "/d", "/deploy", "/base13", "/quiet"],
        "a failure is not running, nor is a session of another workspace"
    );
    tabs.remove("/d");
    tabs.remove("/deploy");
    tabs.insert("/guild-wt", (workspace, true, Status::Idle));
    tabs.insert("/guild", (workspace, true, Status::Done));
    assert_eq!(
        shown(&last_turn, &tabs),
        ["/guild/.git", "/deploy", "/base13", "/quiet", "/d"],
        "a closed tab is no session; the group keeps its place through its other rows"
    );
    tabs.insert("/d", (workspace, true, Status::Done));
    assert_eq!(
        shown(&last_turn, &tabs),
        ["/guild/.git", "/d", "/deploy", "/base13", "/quiet"]
    );
    tabs.insert("/base13", (workspace, true, Status::Working));
    assert_eq!(
        shown(&last_turn, &tabs),
        ["/guild/.git", "/d", "/base13", "/deploy", "/quiet"],
        "back after a failure or a closed tab: the end of the block, in turn"
    );
}

/// The other modes do not look at what is active.
#[test]
fn only_recent_activity_lifts_active_groups() {
    use guild_desktop::column::sort_groups;
    use guild_session::project_sort::{Activity, Mode};
    let ids = [ProjectId::new(), ProjectId::new()];
    let roots = [
        opened(ids[0], Path::new("/a"), None),
        opened(ids[1], Path::new("/b"), None),
    ];
    let manual = groups(&roots, &HashMap::new());
    let stats = |row: &Row| match row {
        Row::Opened { project, .. } if *project == ids[0] => Activity {
            turns: 5,
            last_activity: Some(5),
            last_user: Some(5),
            last_turn: Some(5),
        },
        _ => Activity::default(),
    };
    let active = |group: &Group| (group.key == Path::new("/b")).then_some(1);
    for (mode, first) in [
        (Mode::RecentActivity, "/b"),
        (Mode::MostActivity, "/a"),
        (Mode::LastUserMessage, "/a"),
        (Mode::Manual, "/a"),
    ] {
        let mut found = manual.clone();
        sort_groups(&mut found, mode, stats, active);
        assert_eq!(found[0].key, Path::new(first), "{mode:?}");
    }
}
