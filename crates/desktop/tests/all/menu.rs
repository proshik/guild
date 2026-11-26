//! What the right button offers, per target. Read without a window: the
//! entries are a list, and the window turns them into the component's
//! menu.

use guild_desktop::i18n::Text;
use guild_desktop::view::menu::{
    ChangeDeed, Choice, Deed, Entry, Label, LogDeed, Mark, PathDeed, Subject, TabDeed, TreeDeed,
    WorkspaceDeed, WorktreeDeed, check_column, entries, loses, step,
};
use guild_session::layout::{Axis, Direction};

/// Every deed a menu offers, submenus walked into, in the order a person
/// meets them.
fn deeds(entries: &[Entry]) -> Vec<Deed> {
    let mut found = Vec::new();
    for entry in entries {
        match entry {
            Entry::Item(_, deed) | Entry::NamedItem(_, deed) => found.push(*deed),
            Entry::Choice(choice) => found.push(choice.deed),
            Entry::Submenu(_, inner) => found.extend(deeds(inner)),
            Entry::Separator | Entry::Heading(_) | Entry::Note(_) => {}
        }
    }
    found
}

#[test]
fn a_workspace_menu_is_rename_change_icon_and_delete() {
    assert_eq!(
        deeds(&entries(Subject::Workspace { last: false })),
        [
            Deed::Workspace(WorkspaceDeed::Rename),
            Deed::Workspace(WorkspaceDeed::ChangeIcon),
            Deed::Workspace(WorkspaceDeed::Delete),
        ]
    );
    assert_eq!(
        deeds(&entries(Subject::Workspace { last: true })),
        [
            Deed::Workspace(WorkspaceDeed::Rename),
            Deed::Workspace(WorkspaceDeed::ChangeIcon),
        ],
        "the last workspace cannot be deleted"
    );
    assert!(loses(Deed::Workspace(WorkspaceDeed::Delete)));
}

/// The labels of the top level only — what a person sees before opening
/// anything.
fn labels(entries: &[Entry]) -> Vec<Text> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Item(text, _) | Entry::Submenu(text, _) => Some(*text),
            Entry::NamedItem(_, _) | Entry::Choice(_) => None,
            Entry::Separator | Entry::Heading(_) | Entry::Note(_) => None,
        })
        .collect()
}

/// A file in the tree: what can be made here, what the row is for, where
/// to look inside it, what changes it, what git says, what can be copied.
#[test]
fn a_file_in_the_tree_offers_making_opening_changing_and_copying() {
    let items = entries(Subject::TreeFile);
    assert_eq!(
        deeds(&items),
        [
            Deed::Tree(TreeDeed::NewFile),
            Deed::Tree(TreeDeed::NewDirectory),
            Deed::Tree(TreeDeed::Open),
            Deed::Path(PathDeed::Reveal),
            Deed::Tree(TreeDeed::Terminal),
            Deed::Tree(TreeDeed::FindInFiles),
            Deed::Tree(TreeDeed::Move),
            Deed::Tree(TreeDeed::Cut),
            Deed::Tree(TreeDeed::Copy),
            Deed::Tree(TreeDeed::Paste),
            Deed::Tree(TreeDeed::Rename),
            Deed::Tree(TreeDeed::Delete),
            Deed::Path(PathDeed::ShowDiff),
            Deed::Tree(TreeDeed::Annotate),
            Deed::Tree(TreeDeed::AddToVcs),
            Deed::Path(PathDeed::CopyPath),
            Deed::Path(PathDeed::CopyFullPath),
        ]
    );
    assert!(
        items.contains(&Entry::Item(Text::MenuOpen, Deed::Tree(TreeDeed::Open))),
        "opening is what a file is for"
    );
    assert!(
        items.contains(&Entry::Separator),
        "the groups are ruled off"
    );
}

/// **The destructive item sits inside the transform group.**
///
/// The common arrangement, and the reason is this: at the foot
/// of a menu is where a stray click lands, and Delete is the one item in
/// this menu that touches a person's files. Next to *Rename…* it is
/// reached deliberately.
#[test]
fn delete_is_next_to_rename_and_never_last() {
    for subject in [
        Subject::TreeFile,
        Subject::TreeDirectory {
            expanded: false,
            root: false,
        },
    ] {
        let items = entries(subject);
        let names = labels(&items);
        let rename = names
            .iter()
            .position(|text| *text == Text::MenuRename)
            .expect("a row can be renamed");
        assert_eq!(
            names.get(rename + 1),
            Some(&Text::MenuDelete),
            "Delete follows Rename…"
        );
        assert_ne!(
            names.last(),
            Some(&Text::MenuDelete),
            "and it is not the last thing in the menu"
        );
    }
}

/// **A row under External Libraries offers nothing that writes.**
///
/// A crate unpacked under `~/.cargo/registry` is not a person's own
/// file: offering to rename or delete one would offer to break every
/// build on the machine, and the tree's own menu offers all three. What
/// is left is what a person wants there.
#[test]
fn a_dependency_is_read_and_never_written() {
    for is_dir in [false, true] {
        let items = entries(Subject::Library {
            is_dir,
            expanded: false,
        });
        let offered = deeds(&items);
        for forbidden in [
            Deed::Tree(TreeDeed::NewFile),
            Deed::Tree(TreeDeed::NewDirectory),
            Deed::Tree(TreeDeed::Move),
            Deed::Tree(TreeDeed::Cut),
            Deed::Tree(TreeDeed::Copy),
            Deed::Tree(TreeDeed::Paste),
            Deed::Tree(TreeDeed::Rename),
            Deed::Tree(TreeDeed::Delete),
            Deed::Tree(TreeDeed::AddToVcs),
        ] {
            assert!(
                !offered.contains(&forbidden),
                "{forbidden:?} is offered on somebody else's source"
            );
        }
        assert_eq!(
            offered.first(),
            Some(&Deed::Tree(if is_dir {
                TreeDeed::ToggleDirectory
            } else {
                TreeDeed::Open
            }))
        );
        // The short path is relative to a root that is not the project,
        // so it would paste as a lie; only the full one is offered.
        assert!(offered.contains(&Deed::Path(PathDeed::CopyFullPath)));
        assert!(!offered.contains(&Deed::Path(PathDeed::CopyPath)));
    }
}

/// A directory has nothing to open and no diff: it folds.
#[test]
fn a_directory_folds_and_says_which_way() {
    let closed = entries(Subject::TreeDirectory {
        expanded: false,
        root: false,
    });
    assert_eq!(
        deeds(&closed),
        [
            Deed::Tree(TreeDeed::NewFile),
            Deed::Tree(TreeDeed::NewDirectory),
            Deed::Tree(TreeDeed::ToggleDirectory),
            Deed::Path(PathDeed::Reveal),
            Deed::Tree(TreeDeed::Terminal),
            Deed::Tree(TreeDeed::FindInFiles),
            Deed::Tree(TreeDeed::Move),
            Deed::Tree(TreeDeed::Cut),
            Deed::Tree(TreeDeed::Copy),
            Deed::Tree(TreeDeed::Paste),
            Deed::Tree(TreeDeed::Rename),
            Deed::Tree(TreeDeed::Delete),
            Deed::Tree(TreeDeed::AddToVcs),
            Deed::Path(PathDeed::CopyPath),
            Deed::Path(PathDeed::CopyFullPath),
        ],
        "a directory has no diff of its own and nothing to annotate"
    );

    let open = entries(Subject::TreeDirectory {
        expanded: true,
        root: false,
    });
    assert!(
        open.contains(&Entry::Item(
            Text::MenuCollapse,
            Deed::Tree(TreeDeed::ToggleDirectory)
        )),
        "the item says what pressing it will do, not what the row is"
    );
    assert!(closed.contains(&Entry::Item(
        Text::MenuExpand,
        Deed::Tree(TreeDeed::ToggleDirectory)
    )));
}

/// **The project's own row offers neither Rename nor Delete.**
///
/// It is a directory like any other as far as the menu can see, and its
/// path is the empty one — so *Delete* would hand the directory the
/// window has open to the Trash and *Rename…* would rename it. No
/// confirmation makes either a reasonable thing to offer, and the
/// keyboard route has refused the delete since it was written; this is
/// the same refusal on the route that draws.
///
/// Everything else a directory offers it still offers: a file may be
/// made in the project's root, it may be searched, it folds, and its
/// path may be copied.
#[test]
fn the_projects_own_row_offers_neither_rename_nor_delete() {
    for expanded in [false, true] {
        let items = entries(Subject::TreeDirectory {
            expanded,
            root: true,
        });
        let offered = deeds(&items);
        assert!(
            !offered.contains(&Deed::Tree(TreeDeed::Delete)),
            "the open project is not a row to move to the Trash: {offered:?}"
        );
        assert!(
            !offered.contains(&Deed::Tree(TreeDeed::Rename)),
            "nor one to rename: {offered:?}"
        );
        assert!(
            offered.contains(&Deed::Tree(TreeDeed::NewFile)),
            "and everything that does not write to it stays: {offered:?}"
        );
        assert!(offered.contains(&Deed::Tree(TreeDeed::FindInFiles)));
        assert!(offered.contains(&Deed::Path(PathDeed::CopyFullPath)));
        assert!(
            !items
                .windows(2)
                .any(|pair| pair[0] == Entry::Separator && pair[1] == Entry::Separator),
            "and the rule that separated the group it lost goes with it: {items:?}"
        );
    }
}

/// A tab in the middle of a full strip offers everything: the three
/// closings, both sides, and the four things a path can do.
#[test]
fn a_tab_in_the_middle_offers_closing_both_sides_and_the_path() {
    let middle = entries(Subject::Tab {
        has_file: true,
        session: false,
        index: 1,
        of: 3,
        neighbours: [false; 4],
    });
    assert_eq!(
        deeds(&middle),
        [
            Deed::Tab(TabDeed::Close),
            Deed::Tab(TabDeed::CloseOthers),
            Deed::Tab(TabDeed::CloseAll),
            Deed::Tab(TabDeed::CloseLeft),
            Deed::Tab(TabDeed::CloseRight),
            Deed::Tab(TabDeed::MoveToNewArea(Axis::Horizontal)),
            Deed::Tab(TabDeed::MoveToNewArea(Axis::Vertical)),
            Deed::Tab(TabDeed::SplitRight),
            Deed::Tab(TabDeed::SplitDown),
            Deed::Path(PathDeed::ShowDiff),
            Deed::Tab(TabDeed::CopyFileName),
            Deed::Path(PathDeed::CopyPath),
            Deed::Tab(TabDeed::CopyPathWithLine),
            Deed::Path(PathDeed::CopyFullPath),
            Deed::Path(PathDeed::Reveal)
        ]
    );
}

/// An item that would do nothing makes a person doubt the menu. The
/// first tab has nothing to its left, the last nothing to its right, and
/// a project's only tab has no others to close — one place in a row
/// answers all three.
#[test]
fn a_tab_at_an_end_is_not_offered_the_side_that_is_empty() {
    let first = deeds(&entries(Subject::Tab {
        has_file: true,
        session: false,
        index: 0,
        of: 3,
        neighbours: [false; 4],
    }));
    assert!(!first.contains(&Deed::Tab(TabDeed::CloseLeft)));
    assert!(first.contains(&Deed::Tab(TabDeed::CloseRight)));

    let last = deeds(&entries(Subject::Tab {
        has_file: true,
        session: false,
        index: 2,
        of: 3,
        neighbours: [false; 4],
    }));
    assert!(last.contains(&Deed::Tab(TabDeed::CloseLeft)));
    assert!(!last.contains(&Deed::Tab(TabDeed::CloseRight)));

    let alone = deeds(&entries(Subject::Tab {
        has_file: true,
        session: false,
        index: 0,
        of: 1,
        neighbours: [false; 4],
    }));
    assert!(!alone.contains(&Deed::Tab(TabDeed::CloseOthers)));
    assert!(!alone.contains(&Deed::Tab(TabDeed::CloseLeft)));
    assert!(!alone.contains(&Deed::Tab(TabDeed::CloseRight)));
}

/// A tab with no file — a session, or the diff of a whole tree — offers
/// only what a tab can do, and copies nothing: there is no path to copy
/// and no line to put after one.
#[test]
fn a_tab_with_no_file_offers_no_path() {
    let items = entries(Subject::Tab {
        has_file: false,
        session: false,
        index: 0,
        of: 2,
        neighbours: [false; 4],
    });
    assert_eq!(
        deeds(&items),
        [
            Deed::Tab(TabDeed::Close),
            Deed::Tab(TabDeed::CloseOthers),
            Deed::Tab(TabDeed::CloseAll),
            Deed::Tab(TabDeed::CloseRight),
            Deed::Tab(TabDeed::MoveToNewArea(Axis::Horizontal)),
            Deed::Tab(TabDeed::MoveToNewArea(Axis::Vertical)),
            Deed::Tab(TabDeed::SplitRight),
            Deed::Tab(TabDeed::SplitDown),
        ]
    );
    // The closings are one group, and *Move to Area ▸* is set apart from
    // them by a rule, as the mockup draws it (M12.8, decision 21); the
    // two splits join that group, with no rule of their own.
    assert_eq!(
        items
            .iter()
            .filter(|entry| **entry == Entry::Separator)
            .count(),
        1,
        "one rule, between the closings and where the tab can go: {items:?}"
    );
}

/// Decision 21: a tab divides from its own menu, whatever it shows.
#[test]
fn a_tab_splits_from_its_own_menu() {
    for has_file in [false, true] {
        let found = deeds(&entries(Subject::Tab {
            has_file,
            session: !has_file,
            index: 0,
            of: 1,
            neighbours: [false; 4],
        }));
        assert!(found.contains(&Deed::Tab(TabDeed::SplitRight)));
        assert!(found.contains(&Deed::Tab(TabDeed::SplitDown)));
    }
}

/// Decisions 8 and 21: *Move to Area ▸* names the other areas by where
/// they are — the mockup's *Right Area* — then a rule, then the two that
/// make a new area and move the tab into it.
#[test]
fn a_tab_moves_to_the_areas_that_are_there_or_to_a_new_one() {
    let items = entries(Subject::Tab {
        has_file: false,
        session: true,
        index: 0,
        of: 1,
        neighbours: [false, true, false, true],
    });
    let submenu = items
        .iter()
        .find_map(|e| match e {
            Entry::Submenu(Text::MenuMoveToArea, rows) => Some(rows.clone()),
            _ => None,
        })
        .expect("Move to Area is there");
    assert_eq!(
        submenu,
        vec![
            Entry::Item(
                Text::MenuAreaRight,
                Deed::Tab(TabDeed::MoveToward(Direction::Right))
            ),
            Entry::Item(
                Text::MenuAreaBelow,
                Deed::Tab(TabDeed::MoveToward(Direction::Down))
            ),
            Entry::Separator,
            Entry::Item(
                Text::MenuNewAreaRight,
                Deed::Tab(TabDeed::MoveToNewArea(Axis::Horizontal))
            ),
            Entry::Item(
                Text::MenuNewAreaBelow,
                Deed::Tab(TabDeed::MoveToNewArea(Axis::Vertical))
            ),
        ]
    );

    let alone = entries(Subject::Tab {
        has_file: false,
        session: true,
        index: 0,
        of: 1,
        neighbours: [false; 4],
    });
    let found = deeds(&alone);
    assert!(found.contains(&Deed::Tab(TabDeed::MoveToNewArea(Axis::Horizontal))));
    assert!(
        !found
            .iter()
            .any(|d| matches!(d, Deed::Tab(TabDeed::MoveToward(_))))
    );
}

/// **A session's tab can be renamed, and nothing else's can** — M12.1
/// task 1. What the tab is called is what it *is*, so it comes first; a
/// file is its file's name and a diff its files', and a second name for
/// either is a name that lies.
#[test]
fn a_session_tab_offers_a_name_first_and_a_file_tab_does_not() {
    let session = entries(Subject::Tab {
        has_file: false,
        session: true,
        index: 0,
        of: 2,
        neighbours: [false; 4],
    });
    assert_eq!(deeds(&session)[0], Deed::Tab(TabDeed::Rename));
    assert_eq!(session[1], Entry::Separator);

    let file = deeds(&entries(Subject::Tab {
        has_file: true,
        session: false,
        index: 0,
        of: 2,
        neighbours: [false; 4],
    }));
    assert!(!file.contains(&Deed::Tab(TabDeed::Rename)));
}

/// A change offers its three diffs — against `HEAD`, against the index,
/// and the index against the disk — which is the only door those last
/// two have had since M8.3.1 took the panel's three groups away.
#[test]
fn a_change_offers_the_three_diffs() {
    let items = entries(Subject::Change {
        ticked: true,
        unmerged: false,
        is_dir: false,
        unversioned: false,
        gone: false,
    });
    assert_eq!(
        deeds(&items),
        [
            Deed::Path(PathDeed::ShowDiff),
            Deed::Change(ChangeDeed::ShowDiffStaged),
            Deed::Change(ChangeDeed::ShowDiffUnstaged),
            Deed::Change(ChangeDeed::ToggleTick),
            Deed::Change(ChangeDeed::Rollback),
            Deed::Change(ChangeDeed::Delete),
            Deed::Path(PathDeed::CopyPath),
            Deed::Path(PathDeed::CopyFullPath),
            Deed::Path(PathDeed::Reveal)
        ]
    );
    assert!(
        items.contains(&Entry::Item(
            Text::MenuExclude,
            Deed::Change(ChangeDeed::ToggleTick)
        )),
        "a ticked row offers to be left out"
    );
    assert!(
        entries(Subject::Change {
            ticked: false,
            unmerged: false,
            is_dir: false,
            unversioned: false,
            gone: false,
        })
        .contains(&Entry::Item(
            Text::MenuInclude,
            Deed::Change(ChangeDeed::ToggleTick)
        )),
        "and an unticked one offers to be put in"
    );
}

/// **The two git items point opposite ways over one fact.** A file git
/// has never seen is offered *Add to VCS* and no rollback — there is
/// nothing in `HEAD` or in the index for it to go back to, and git
/// refuses the path outright. A tracked change is offered the rollback
/// and no *Add to VCS*: it is already in.
#[test]
fn add_to_vcs_and_rollback_never_appear_together() {
    let unversioned = deeds(&entries(Subject::Change {
        ticked: false,
        unmerged: false,
        is_dir: false,
        unversioned: true,
        gone: false,
    }));
    assert!(unversioned.contains(&Deed::Change(ChangeDeed::AddToVcs)));
    assert!(!unversioned.contains(&Deed::Change(ChangeDeed::Rollback)));

    let tracked = deeds(&entries(Subject::Change {
        ticked: false,
        unmerged: false,
        is_dir: false,
        unversioned: false,
        gone: false,
    }));
    assert!(tracked.contains(&Deed::Change(ChangeDeed::Rollback)));
    assert!(!tracked.contains(&Deed::Change(ChangeDeed::AddToVcs)));

    // Whichever of the two it is, the Trash is offered beside it: what
    // *Delete* does is true of a tracked file and of an unversioned one
    // in exactly the same way.
    for items in [&unversioned, &tracked] {
        assert!(items.contains(&Deed::Change(ChangeDeed::Delete)));
    }
}

/// A conflict is never ticked, so nothing offers to tick it; a
/// directory git did not descend into has no diff to show.
#[test]
fn a_conflict_and_a_directory_offer_less() {
    let conflict = entries(Subject::Change {
        ticked: false,
        unmerged: true,
        is_dir: false,
        unversioned: false,
        gone: false,
    });
    assert!(!deeds(&conflict).contains(&Deed::Change(ChangeDeed::ToggleTick)));
    assert!(deeds(&conflict).contains(&Deed::Path(PathDeed::ShowDiff)));
    // **Nor to roll it back.** git does not refuse a restore of an
    // unmerged path — it succeeds, writes `HEAD`'s side into the index
    // and the worktree, and thereby resolves the conflict one way with
    // the merge still standing. *Rollback* does not say that, so the
    // guard is ours; `changes::rollback_of` refuses it too.
    assert!(
        !deeds(&conflict).contains(&Deed::Change(ChangeDeed::Rollback)),
        "resolving a conflict is not what the word Rollback means"
    );

    let directory = entries(Subject::Change {
        ticked: false,
        unmerged: false,
        is_dir: true,
        unversioned: true,
        gone: false,
    });
    assert!(!deeds(&directory).contains(&Deed::Path(PathDeed::ShowDiff)));
    assert!(deeds(&directory).contains(&Deed::Path(PathDeed::CopyPath)));
    assert!(
        deeds(&directory).contains(&Deed::Change(ChangeDeed::Delete)),
        "an untracked directory is still a thing the Trash can take"
    );
}

/// **A row git marks `D` is not offered the Trash.** Its file is by
/// definition no longer on the disk, so the item would open a warning
/// sheet whose only possible ending is the platform refusing a path that
/// is not there — the smallest shape of a state a person can reach and
/// not leave. Everything else the row can answer it still answers.
#[test]
fn a_row_whose_file_is_already_gone_is_not_offered_the_trash() {
    let gone = entries(Subject::Change {
        ticked: true,
        unmerged: false,
        is_dir: false,
        unversioned: false,
        gone: true,
    });
    assert!(
        !deeds(&gone).contains(&Deed::Change(ChangeDeed::Delete)),
        "the Trash has nothing to take: {:?}",
        deeds(&gone)
    );
    assert!(
        deeds(&gone).contains(&Deed::Change(ChangeDeed::Rollback)),
        "and a deleted file is exactly the one a person rolls back"
    );
    assert!(deeds(&gone).contains(&Deed::Path(PathDeed::ShowDiff)));
}

/// A section head is about the whole panel: the four things its toolbar
/// does, where the hand already is.
#[test]
fn a_section_head_offers_what_the_toolbar_does() {
    assert_eq!(
        deeds(&entries(Subject::ChangeSection)),
        [
            Deed::Change(ChangeDeed::TickAll),
            Deed::Change(ChangeDeed::UntickAll),
            Deed::Change(ChangeDeed::CollapseSections),
            Deed::Change(ChangeDeed::ExpandSections)
        ]
    );
}

/// A branch offers to be listed, and the one already listed offers the
/// way back instead. Neither offers to check anything out: decision 3
/// of M8.3.2, which is spec decision 18 still standing.
#[test]
fn a_branch_offers_a_listing_and_never_a_checkout() {
    let other = entries(Subject::Branch { listed: false });
    assert_eq!(
        deeds(&other),
        [
            Deed::Log(LogDeed::ShowBranchLog),
            Deed::Log(LogDeed::CopyBranchName)
        ],
        "and nothing that moves HEAD"
    );

    let listed = entries(Subject::Branch { listed: true });
    assert_eq!(
        deeds(&listed),
        [
            Deed::Log(LogDeed::ShowHeadLog),
            Deed::Log(LogDeed::CopyBranchName)
        ]
    );
    assert!(listed.contains(&Entry::Item(
        Text::MenuShowHeadLog,
        Deed::Log(LogDeed::ShowHeadLog)
    )));
}

/// A commit offers what a person copies out of a log: the revision, in
/// both lengths, and the message.
#[test]
fn a_commit_offers_its_revision_and_its_message() {
    assert_eq!(
        deeds(&entries(Subject::Commit)),
        [
            Deed::Log(LogDeed::CopyShortRevision),
            Deed::Log(LogDeed::CopyRevision),
            Deed::Log(LogDeed::CopySubject)
        ]
    );
}

/// **An item may show a stroke only when the action would do the same
/// thing to the same target.**
///
/// Every item is a deed on the row under the pointer and every action
/// reaches the thing in front — decision 20 of the spec — so a menu that
/// showed `cmd-w` beside *Close* on a background tab would be promising
/// something the key does not do. This is the one place that rule is
/// written, and this is what pins it.
#[test]
fn a_tab_shows_a_stroke_only_when_the_key_would_do_the_same() {
    use guild_desktop::actions::keymap::Named;
    use guild_desktop::view::menu::stroke_for;

    let close = Deed::Tab(TabDeed::Close);
    assert_eq!(
        stroke_for(close, true),
        Some(Named::CloseTab),
        "on the tab in front, cmd-w closes exactly this tab"
    );
    assert_eq!(
        stroke_for(close, false),
        None,
        "on a background tab it would close the wrong file, so it is not shown"
    );

    let others = Deed::Tab(TabDeed::CloseOthers);
    assert_eq!(stroke_for(others, true), Some(Named::CloseOtherTabs));
    assert_eq!(
        stroke_for(others, false),
        None,
        "the action keeps the tab in front, not the one under the pointer"
    );

    // Since M12.8 a tab's *Close All* closes its own area's tabs and the
    // stroke closes the focused area's: the same only on the tab in front.
    assert_eq!(
        stroke_for(Deed::Tab(TabDeed::CloseAll), true),
        Some(Named::CloseAllTabs),
    );
    assert_eq!(
        stroke_for(Deed::Tab(TabDeed::CloseAll), false),
        None,
        "on another area's tab the stroke would close the wrong strip"
    );
}

/// **The splits print their strokes where the strokes do the same** —
/// decisions 14 and 21: `ctrl-cmd-alt-right` divides the focused pane of
/// the tab in front, which is what *Split Tab Right* does on that tab and
/// not on a tab behind it.
#[test]
fn a_tab_s_splits_show_their_strokes_only_in_front() {
    use guild_desktop::actions::keymap::Named;
    use guild_desktop::view::menu::stroke_for;

    for (deed, named) in [
        (TabDeed::SplitRight, Named::SplitPaneRight),
        (TabDeed::SplitDown, Named::SplitPaneDown),
    ] {
        assert_eq!(stroke_for(Deed::Tab(deed), true), Some(named));
        assert_eq!(stroke_for(Deed::Tab(deed), false), None, "{deed:?}");
    }
}

/// **`cmd-w` closes a pane of a divided tab, not the tab** — decision 14 —
/// so *Close* on the tab in front shows no stroke while it is divided:
/// the key would leave the tab open with one pane fewer. Every other row
/// keeps its stroke, because every other stroke still does what its row
/// does.
#[test]
fn a_divided_tab_s_close_shows_no_stroke() {
    use guild_desktop::actions::keymap::Named;
    use guild_desktop::view::menu::{Front, stroke_shown};

    let divided = Front {
        front: true,
        divided: true,
    };
    let whole = Front {
        front: true,
        divided: false,
    };
    let close = Deed::Tab(TabDeed::Close);
    assert_eq!(stroke_shown(close, whole), Some(Named::CloseTab));
    assert_eq!(stroke_shown(close, divided), None);
    for (deed, named) in [
        (TabDeed::CloseOthers, Named::CloseOtherTabs),
        (TabDeed::CloseAll, Named::CloseAllTabs),
        (TabDeed::SplitRight, Named::SplitPaneRight),
        (TabDeed::SplitDown, Named::SplitPaneDown),
    ] {
        assert_eq!(stroke_shown(Deed::Tab(deed), divided), Some(named));
    }
}

/// The Commit panel's section head is about the whole panel, and so are
/// the four actions: there is no row for them to disagree about.
#[test]
fn the_section_head_shows_the_strokes_its_toolbar_has() {
    use guild_desktop::actions::keymap::Named;
    use guild_desktop::view::menu::stroke_for;

    for (deed, named) in [
        (Deed::Change(ChangeDeed::TickAll), Named::TickAllChanges),
        (Deed::Change(ChangeDeed::UntickAll), Named::UntickAllChanges),
        (
            Deed::Change(ChangeDeed::CollapseSections),
            Named::CollapseChanges,
        ),
        (
            Deed::Change(ChangeDeed::ExpandSections),
            Named::ExpandChanges,
        ),
    ] {
        assert_eq!(stroke_for(deed, false), Some(named), "{deed:?}");
    }
}

/// **A project's menu** — M12.2 task 9: what it is called, a new worktree
/// of it, where it is, and taking it out of the window, with an icon on
/// every item. Without an open root there is no `HEAD` to start a worktree
/// from, so that item alone goes.
#[test]
fn a_project_offers_its_name_a_worktree_its_folder_and_its_removal() {
    use guild_desktop::view::menu::{ProjectDeed, icon_of};

    let items = entries(Subject::Repository { opened: true });
    assert_eq!(
        items,
        [
            Entry::Item(Text::MenuRenameProject, Deed::Project(ProjectDeed::Rename)),
            Entry::Item(
                Text::ColumnNewWorktree,
                Deed::Project(ProjectDeed::NewWorktree)
            ),
            Entry::Separator,
            Entry::Item(Text::MenuCopyPath, Deed::Project(ProjectDeed::CopyPath)),
            Entry::Item(Text::MenuReveal, Deed::Project(ProjectDeed::Reveal)),
            Entry::Separator,
            Entry::Item(Text::MenuRemoveProject, Deed::Project(ProjectDeed::Remove)),
        ]
    );
    for deed in deeds(&items) {
        assert!(icon_of(deed).is_some(), "{deed:?} has no icon");
    }
    assert!(guild_desktop::view::menu::loses(Deed::Project(
        ProjectDeed::Remove
    )));

    let closed = deeds(&entries(Subject::Repository { opened: false }));
    assert!(!closed.contains(&Deed::Project(ProjectDeed::NewWorktree)));

    // A row's menu carries no icons.
    assert_eq!(icon_of(Deed::Path(PathDeed::CopyPath)), None);
}

/// A worktree whose branch has a merge request offers it as a group of
/// its own after the work: View MR and Copy MR URL — GLD-063. Without
/// one, neither is there.
#[test]
fn a_worktree_with_a_merge_request_offers_it_after_the_work() {
    use guild_desktop::view::menu::WorktreeDeed;
    let items = entries(Subject::Worktree {
        primary: false,
        opened: true,
        locked: false,
        mr: true,
    });
    assert_eq!(
        deeds(&items),
        [
            Deed::Column(WorktreeDeed::Run),
            Deed::Column(WorktreeDeed::Terminal),
            Deed::Column(WorktreeDeed::ViewMr),
            Deed::Column(WorktreeDeed::CopyMrUrl),
            Deed::Column(WorktreeDeed::CopyPath),
            Deed::Column(WorktreeDeed::Reveal),
            Deed::Column(WorktreeDeed::Remove),
        ]
    );
    // `cmd-alt-g` opens the active worktree's MR, so the row in front
    // shows it and another row does not.
    let view = Deed::Column(WorktreeDeed::ViewMr);
    assert_eq!(
        guild_desktop::view::menu::stroke_for(view, true),
        Some(guild_desktop::actions::keymap::Named::ViewMergeRequest)
    );
    assert_eq!(guild_desktop::view::menu::stroke_for(view, false), None);
    assert_eq!(
        guild_desktop::view::menu::stroke_for(Deed::Column(WorktreeDeed::CopyMrUrl), true),
        None
    );
    // An unopened worktree's MR is offered too: the MR is the branch's.
    assert_eq!(
        deeds(&entries(Subject::Worktree {
            primary: false,
            opened: false,
            locked: false,
            mr: true,
        })),
        [
            Deed::Column(WorktreeDeed::ViewMr),
            Deed::Column(WorktreeDeed::CopyMrUrl),
            Deed::Column(WorktreeDeed::CopyPath),
            Deed::Column(WorktreeDeed::Reveal),
            Deed::Column(WorktreeDeed::Remove),
        ]
    );
}

/// A worktree's row is a working place, not a label — M12 task 15. What
/// a person came for is first: its check and a shell in it. Then the
/// folder, which every row has. Then what takes the row away, which is
/// last because it is the one that cannot be undone by doing it again.
///
/// **One way out of a row** — the person,
/// 2026-09-22. *Close Worktree* sat beside *Remove Worktree…* and read as
/// its twin; it is gone, and the primary, which git will not remove,
/// offers neither: the repository leaves the window by *Remove Project*.
#[test]
fn a_worktree_offers_the_work_first_and_the_removal_last() {
    use guild_desktop::view::menu::WorktreeDeed;

    let items = entries(Subject::Worktree {
        primary: false,
        opened: true,
        locked: false,
        mr: false,
    });
    assert_eq!(
        deeds(&items),
        [
            Deed::Column(WorktreeDeed::Run),
            Deed::Column(WorktreeDeed::Terminal),
            Deed::Column(WorktreeDeed::CopyPath),
            Deed::Column(WorktreeDeed::Reveal),
            Deed::Column(WorktreeDeed::Remove),
        ]
    );

    // The repository's own checkout is not a worktree git will remove —
    // `column::removal` refuses it — so the item is not there to choose.
    let primary = entries(Subject::Worktree {
        primary: true,
        opened: true,
        locked: false,
        mr: false,
    });
    assert_eq!(
        deeds(&primary),
        [
            Deed::Column(WorktreeDeed::Run),
            Deed::Column(WorktreeDeed::Terminal),
            Deed::Column(WorktreeDeed::CopyPath),
            Deed::Column(WorktreeDeed::Reveal),
        ]
    );

    // One git lists that nobody has opened has no watcher, no git thread
    // and no runner behind it: the folder is all it can offer, and the
    // removal, which is git's own work and not the window's.
    assert_eq!(
        deeds(&entries(Subject::Worktree {
            primary: false,
            opened: false,
            locked: false,
            mr: false,
        })),
        [
            Deed::Column(WorktreeDeed::CopyPath),
            Deed::Column(WorktreeDeed::Reveal),
            Deed::Column(WorktreeDeed::Remove),
        ]
    );
}

/// Everything else shows nothing, and the nearest miss is named: the
/// diff of a change row has an action, and that action acts on the row
/// the keyboard is on rather than the row the pointer is on.
#[test]
fn a_deed_with_no_honest_stroke_shows_none() {
    use guild_desktop::view::menu::stroke_for;

    for deed in [
        Deed::Path(PathDeed::ShowDiff),
        Deed::Path(PathDeed::CopyPath),
        Deed::Path(PathDeed::CopyFullPath),
        Deed::Path(PathDeed::Reveal),
        // `Tree(Open)` left this list when it gained `f4` — its rule is
        // `a_file_shows_f4_only_where_f4_would_open_it`.
        Deed::Tree(TreeDeed::ToggleDirectory),
        Deed::Change(ChangeDeed::ToggleTick),
        Deed::Change(ChangeDeed::ShowDiffStaged),
        Deed::Log(LogDeed::ShowBranchLog),
        Deed::Log(LogDeed::CopyRevision),
        // M8.8.1's four. Neither side of the strip has an action at all — the
        // plan gave `CloseTabsToTheLeft` no stroke because the common IDE
        // keymap gives it none either — and the two copies are about the row
        // the pointer is on, which is exactly what a stroke could not
        // promise.
        Deed::Tab(TabDeed::CloseLeft),
        Deed::Tab(TabDeed::CloseRight),
        Deed::Tab(TabDeed::CopyFileName),
        Deed::Tab(TabDeed::CopyPathWithLine),
        // M12.8's moves have no action: a tab is moved by its menu or by
        // the hand.
        Deed::Tab(TabDeed::MoveToward(Direction::Right)),
        Deed::Tab(TabDeed::MoveToNewArea(Axis::Vertical)),
    ] {
        assert_eq!(stroke_for(deed, true), None, "{deed:?}");
        assert_eq!(stroke_for(deed, false), None, "{deed:?}");
    }
}

/// **A lock is offered to be taken off, and only where there is one** —
/// M12 task 18. git refuses a locked worktree before it looks at
/// anything else, and the lock outlives whoever took it: a dead session's
/// note otherwise refuses every removal for ever, with `remove -f -f` as
/// git's own advice and this repository's one barred spelling.
#[test]
fn a_locked_worktree_offers_to_be_unlocked() {
    use guild_desktop::view::menu::WorktreeDeed;

    let locked = deeds(&entries(Subject::Worktree {
        primary: false,
        opened: true,
        locked: true,
        mr: false,
    }));
    assert!(locked.contains(&Deed::Column(WorktreeDeed::Unlock)));
    // The unlock comes before the removal it makes possible.
    let at = |deed| locked.iter().position(|found| *found == deed);
    assert!(at(Deed::Column(WorktreeDeed::Unlock)) < at(Deed::Column(WorktreeDeed::Remove)));

    // And nothing to unlock is nothing offered.
    let free = deeds(&entries(Subject::Worktree {
        primary: false,
        opened: true,
        locked: false,
        mr: false,
    }));
    assert!(!free.contains(&Deed::Column(WorktreeDeed::Unlock)));
}

/// **The keyboard walks a menu past its separators, and stops at its
/// ends** — M12.2 task 8, now that the menu is drawn by Guild rather than by
/// the component. A separator is not a place to stand; a submenu is.
#[test]
fn the_keyboard_walks_a_menu_past_its_separators() {
    let item = |text| Entry::Item(text, Deed::Path(PathDeed::CopyPath));
    let menu = [
        item(Text::MenuCopyPath),
        Entry::Separator,
        item(Text::MenuReveal),
        Entry::Submenu(Text::MenuOpen, vec![]),
        Entry::Separator,
    ];
    assert_eq!(
        step(&menu, None, true),
        Some(0),
        "down from nothing: the first"
    );
    assert_eq!(
        step(&menu, None, false),
        Some(3),
        "up from nothing: the last"
    );
    assert_eq!(step(&menu, Some(0), true), Some(2), "over the separator");
    assert_eq!(step(&menu, Some(2), true), Some(3), "onto the submenu");
    assert_eq!(step(&menu, Some(3), true), Some(3), "the end is the end");
    assert_eq!(step(&menu, Some(2), false), Some(0));
    assert_eq!(step(&menu, Some(0), false), Some(0));
    assert_eq!(
        step(&[Entry::Separator], None, true),
        None,
        "nothing to stand on"
    );
}

/// **What loses something is drawn as such** — Delete, Remove, Rollback in
/// `accent.rust`, as *Remove Project* is.
#[test]
fn an_item_that_loses_something_says_so() {
    assert!(loses(Deed::Tree(TreeDeed::Delete)));
    assert!(loses(Deed::Change(ChangeDeed::Rollback)));
    assert!(loses(Deed::Change(ChangeDeed::Delete)));
    assert!(loses(Deed::Column(WorktreeDeed::Remove)));
    assert!(!loses(Deed::Path(PathDeed::CopyPath)));
    assert!(
        !loses(Deed::Tab(TabDeed::Close)),
        "a closed tab loses no work"
    );
    assert!(!loses(Deed::Log(LogDeed::CopyRevision)));
}

/// **A terminal's menu is the Edit menu's terminal half** — M5.12 task 13:
/// what a person does with the output, then looking in it, then emptying
/// it. *Copy* only with something to copy: an item that copies nothing is
/// an item that makes a person doubt the menu.
#[test]
fn a_terminal_s_menu_offers_copy_only_with_a_selection() {
    use guild_desktop::view::menu::TerminalDeed;

    assert_eq!(
        entries(Subject::Terminal { selection: true }),
        [
            Entry::Item(Text::MenuCopy, Deed::Terminal(TerminalDeed::Copy)),
            Entry::Item(Text::MenuPaste, Deed::Terminal(TerminalDeed::Paste)),
            Entry::Item(Text::MenuSelectAll, Deed::Terminal(TerminalDeed::SelectAll)),
            Entry::Separator,
            Entry::Item(Text::MenuFind, Deed::Terminal(TerminalDeed::Find)),
            Entry::Separator,
            Entry::Item(Text::MenuClearTerminal, Deed::Terminal(TerminalDeed::Clear)),
        ]
    );
    assert_eq!(
        deeds(&entries(Subject::Terminal { selection: false })),
        [
            Deed::Terminal(TerminalDeed::Paste),
            Deed::Terminal(TerminalDeed::SelectAll),
            Deed::Terminal(TerminalDeed::Find),
            Deed::Terminal(TerminalDeed::Clear),
        ]
    );
    assert!(
        !entries(Subject::Terminal { selection: false })
            .iter()
            .any(|entry| matches!(entry, Entry::Item(Text::MenuCopy, _))),
    );
    for selection in [true, false] {
        assert!(
            deeds(&entries(Subject::Terminal { selection }))
                .into_iter()
                .all(|deed| loses(deed) == matches!(deed, Deed::Terminal(TerminalDeed::Clear))),
            "clearing discards the screen and scrollback; the other terminal items retain them"
        );
    }
}

/// **Strokes on a tab's terminal and none on the Output panel's** — M5.12
/// decision 14. The actions reach the focused terminal, and the menu gives
/// the focus back to the tab: on a tab's terminal that is the terminal the
/// item acted on, and on the Output's it is not.
#[test]
fn a_terminal_s_items_show_strokes_only_where_the_focus_returns() {
    use guild_desktop::actions::keymap::Named;
    use guild_desktop::view::menu::{TerminalDeed, stroke_for};

    for (deed, named) in [
        (TerminalDeed::Copy, Named::Copy),
        (TerminalDeed::Paste, Named::Paste),
        (TerminalDeed::SelectAll, Named::SelectAll),
        (TerminalDeed::Find, Named::Find),
        (TerminalDeed::Clear, Named::ClearTerminal),
    ] {
        assert_eq!(
            stroke_for(Deed::Terminal(deed), true),
            Some(named),
            "{deed:?} on a tab's terminal"
        );
        assert_eq!(
            stroke_for(Deed::Terminal(deed), false),
            None,
            "{deed:?} on the Output panel's terminal"
        );
    }
}

/// **The tree's *Open* shows `f4` only on the selected row.** The right
/// button does not move the selection, and `f4` — *Jump to Source* —
/// opens the selected file; an item that advertised it on any
/// other row would promise a key that opens a different file.
#[test]
fn a_file_shows_f4_only_where_f4_would_open_it() {
    use guild_desktop::actions::keymap::Named;
    use guild_desktop::view::menu::stroke_for;

    let open = Deed::Tree(TreeDeed::Open);
    assert_eq!(stroke_for(open, true), Some(Named::JumpToSource));
    assert_eq!(
        stroke_for(open, false),
        None,
        "another row than the selection"
    );
}

/// A row of a menu that has a current item — M12.7.
fn choice(deed: Deed, enabled: bool) -> Entry {
    Entry::Choice(Choice {
        label: Label::Named("x".into()),
        mark: Mark::None,
        deed,
        check: None,
        enabled,
        detail: None,
        trailing: None,
    })
}

/// **A heading, a disabled row and a footnote are not places** — M12.7.
/// The heading is the dock menu's *Move to*, the footnote the Log filter's
/// *Press ⌫ to clear the filter*: the two shapes the engine takes over.
#[test]
fn the_keyboard_stands_on_items_and_choices_and_nowhere_else() {
    let d = Deed::Tab(TabDeed::Close);
    let list = vec![
        Entry::Heading(Label::Text(Text::MoveTo)),
        choice(d, true),
        choice(d, false),
        Entry::Separator,
        Entry::Item(Text::MenuCloseTab, d),
        Entry::Note(Label::Text(Text::LogClearHint)),
    ];
    assert_eq!(step(&list, None, true), Some(1), "past the heading");
    assert_eq!(
        step(&list, Some(1), true),
        Some(4),
        "past the disabled row and the rule"
    );
    assert_eq!(
        step(&list, Some(4), true),
        Some(4),
        "the footnote is not a place"
    );
    assert_eq!(
        step(&list, None, false),
        Some(4),
        "up from nothing is the last item"
    );
}

/// **An item under choices keeps the check's column** — M12.7: the
/// project popup lists its projects as choices, then a rule, then items,
/// and the items' words have to start where the choices' do.
#[test]
fn every_row_of_a_menu_with_a_current_item_keeps_the_check_column() {
    let d = Deed::Tab(TabDeed::Close);
    let current = Entry::Choice(Choice {
        label: Label::Named("here".into()),
        mark: Mark::None,
        deed: d,
        check: Some(true),
        enabled: true,
        detail: None,
        trailing: None,
    });
    let list = vec![
        Entry::Heading(Label::Text(Text::MoveTo)),
        current,
        choice(d, true),
        Entry::Separator,
        Entry::Item(Text::MenuCloseTab, d),
        Entry::NamedItem("named".into(), d),
        Entry::Submenu(Text::MenuOpen, vec![]),
        Entry::Note(Label::Text(Text::LogClearHint)),
    ];
    assert_eq!(
        check_column(&list),
        [
            None,
            Some(true),
            Some(false),
            None,
            Some(false),
            Some(false),
            Some(false),
            None,
        ]
    );
    // A menu with no current item has no column at all, which is every
    // right-button menu of today.
    assert!(
        check_column(&entries(Subject::TreeFile))
            .iter()
            .all(Option::is_none)
    );
}

/// The Log's three filter menus — M12.7 task 6: the row that takes the
/// filter off first and a rule under it, the chosen value checked and
/// every other choice not, the star on the branch `HEAD` is on whether or
/// not it is chosen, the branches behind *Local ▸*, and the footnote that
/// promises Backspace at every menu's foot.
#[test]
fn the_log_s_filters_clear_first_check_the_chosen_and_end_in_the_hint() {
    use guild_desktop::git::Scope;
    use guild_desktop::view::menu::log_filter::{FilterDeed, FilterList, Since, log_filter};

    /// The choices of a list, in order: its words, its check and its mark.
    fn rows(entries: &[Entry]) -> Vec<(Label, Option<bool>, Mark)> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Choice(choice) => {
                    Some((choice.label.clone(), choice.check, choice.mark.clone()))
                }
                _ => None,
            })
            .collect()
    }
    let named = |name: &str| Label::Named(name.to_string());
    let text = Label::Text;

    let names = |list: &[&str]| list.iter().map(|name| name.to_string()).collect::<Vec<_>>();
    let branch = |chosen: Scope| FilterList::Branch {
        chosen,
        head: Some("m12-7".to_string()),
        default: Some("main".to_string()),
        recent: names(&["m12-6", "m12-7"]),
        local: names(&["m12-6", "m12-7", "main"]),
    };
    let lists = [
        branch(Scope::Named("m12-6".to_string())),
        FilterList::User {
            chosen: Some("Ann".to_string()),
            authors: names(&["Ann", "Bob"]),
        },
        FilterList::Date {
            chosen: Some(Since::Week),
        },
    ];
    for list in &lists {
        let items = log_filter(list);
        let Entry::Choice(first) = &items[0] else {
            panic!("{list:?}: the first row is a choice");
        };
        assert_eq!(
            first.deed,
            Deed::LogFilter(FilterDeed::Clear),
            "{list:?}: the first row takes the filter off"
        );
        assert_eq!(items[1], Entry::Separator, "{list:?}: a rule under it");
        assert_eq!(
            items.last(),
            Some(&Entry::Note(Label::Text(Text::LogClearHint))),
            "{list:?}: the footnote ends every menu"
        );
        let checked = rows(&items)
            .into_iter()
            .filter(|(_, check, _)| *check == Some(true))
            .count();
        assert!(
            rows(&items).iter().all(|(_, check, _)| check.is_some()),
            "{list:?}: every choice keeps the check's column"
        );
        assert_eq!(checked, 1, "{list:?}: the chosen value only");
    }

    // The Branch menu, `m12-6` chosen and `HEAD` on `m12-7`.
    let items = log_filter(&lists[0]);
    assert_eq!(
        rows(&items),
        [
            (text(Text::LogAllBranches), Some(false), Mark::None),
            (text(Text::LogHead), Some(false), Mark::Star),
            (named("main"), Some(false), Mark::None),
            (named("m12-6"), Some(true), Mark::None),
            (named("m12-7"), Some(false), Mark::Star),
        ]
    );
    assert!(items.contains(&Entry::Heading(Label::Text(Text::LogRecentBranches))));
    let Some(Entry::Submenu(Text::LogLocalBranches, local)) = items
        .iter()
        .find(|entry| matches!(entry, Entry::Submenu(..)))
    else {
        panic!("the branches are behind Local ▸");
    };
    assert_eq!(
        rows(local),
        [
            (named("m12-6"), Some(true), Mark::None),
            (named("m12-7"), Some(false), Mark::Star),
            (named("main"), Some(false), Mark::None),
        ]
    );
    // Every branch row names its branch in the list the window resolves
    // a deed against: the deed and the name agree.
    for entry in local {
        let Entry::Choice(choice) = entry else {
            continue;
        };
        let Deed::LogFilter(FilterDeed::Branch(at)) = choice.deed else {
            panic!("a local row lists a branch");
        };
        assert_eq!(
            Some(named(lists[0].branch(at).unwrap())),
            Some(choice.label.clone())
        );
    }

    // Unnarrowed, *All* is the one checked; the star stays on `HEAD`'s.
    let items = log_filter(&branch(Scope::All));
    assert_eq!(rows(&items)[0].1, Some(true));
    assert_eq!(rows(&items)[4], (named("m12-7"), Some(false), Mark::Star));
    let items = log_filter(&branch(Scope::Head));
    assert_eq!(
        rows(&items)[1],
        (text(Text::LogHead), Some(true), Mark::Star)
    );
    // No lately-read branches, no *Recent* heading.
    let bare = FilterList::Branch {
        chosen: Scope::All,
        head: None,
        default: None,
        recent: Vec::new(),
        local: names(&["main"]),
    };
    assert!(
        !log_filter(&bare)
            .iter()
            .any(|entry| matches!(entry, Entry::Heading(_)))
    );

    // User and Date.
    assert_eq!(
        rows(&log_filter(&lists[1])),
        [
            (text(Text::LogAnyone), Some(false), Mark::None),
            (named("Ann"), Some(true), Mark::None),
            (named("Bob"), Some(false), Mark::None),
        ]
    );
    assert_eq!(
        rows(&log_filter(&lists[2])),
        [
            (text(Text::LogAnyTime), Some(false), Mark::None),
            (text(Text::LogLastDay), Some(false), Mark::None),
            (text(Text::LogLastWeek), Some(true), Mark::None),
            (text(Text::LogLastMonth), Some(false), Mark::None),
        ]
    );
    assert_eq!(
        deeds(&log_filter(&lists[2])),
        [
            Deed::LogFilter(FilterDeed::Clear),
            Deed::LogFilter(FilterDeed::Since(Since::Day)),
            Deed::LogFilter(FilterDeed::Since(Since::Week)),
            Deed::LogFilter(FilterDeed::Since(Since::Month)),
        ]
    );
}

/// **Find in Files' history is a menu of the engine** — M12.7 task 7: a
/// heading over what a field remembers, newest first, and a plain choice
/// per entry. No check, no mark, no ✕: a remembered text is not a current
/// value the way a filter's is, and it is not a row a person removes from
/// here — the two reasons `Choice`'s other slots exist and stay empty.
#[test]
fn find_in_files_history_is_a_heading_over_the_remembered_texts_newest_first() {
    use guild_desktop::find_in_files::Field;
    use guild_desktop::view::menu::history::history;

    let items = vec!["needle".to_string(), "haystack".to_string()];
    let entries = history(Field::Text, Text::FindInFilesHistory, &items);

    assert_eq!(
        entries[0],
        Entry::Heading(Label::Text(Text::FindInFilesHistory)),
        "the field's own title, over the list"
    );
    assert_eq!(
        deeds(&entries),
        [
            Deed::UseHistory(Field::Text, 0),
            Deed::UseHistory(Field::Text, 1),
        ],
        "newest first — the order the history is already kept in"
    );

    let Entry::Choice(first) = &entries[1] else {
        panic!("a remembered text is a choice");
    };
    assert_eq!(first.label, Label::Named("needle".to_string()));
    assert_eq!(first.mark, Mark::None);
    assert_eq!(first.check, None, "not a current value");
    assert_eq!(first.detail, None);
    assert_eq!(first.trailing, None, "not a row that is removed from here");
    assert!(first.enabled);

    // The heading stands even with nothing remembered — a field a person
    // has never used still has a chevron that opens something.
    assert_eq!(
        history(Field::Mask, Text::FindInFilesMaskHistory, &[]),
        [Entry::Heading(Label::Text(Text::FindInFilesMaskHistory))]
    );

    // The deed carries which field the menu was about, not only the
    // place: `Field::Mask` and `Field::Text` at the same place are two
    // different rows.
    assert_eq!(
        deeds(&history(Field::Mask, Text::FindInFilesMaskHistory, &items)),
        [
            Deed::UseHistory(Field::Mask, 0),
            Deed::UseHistory(Field::Mask, 1),
        ]
    );
}

#[test]
fn explorer_exposes_file_clipboard_and_move_but_libraries_do_not() {
    fn labels(items: &[Entry], out: &mut Vec<Text>) {
        for item in items {
            match item {
                Entry::Item(label, _) => out.push(*label),
                Entry::Submenu(_, children) => labels(children, out),
                _ => {}
            }
        }
    }
    for subject in [
        Subject::TreeFile,
        Subject::TreeDirectory {
            expanded: false,
            root: false,
        },
    ] {
        let mut found = Vec::new();
        labels(&entries(subject), &mut found);
        for label in [
            Text::MenuMove,
            Text::MenuCutFiles,
            Text::MenuCopyFiles,
            Text::MenuPasteFiles,
        ] {
            assert!(found.contains(&label), "missing {label:?}");
        }
    }
}

#[test]
fn destructive_action_rows_and_terminal_history_clear_are_losing() {
    use guild_desktop::actions::keymap::Named;
    use guild_desktop::view::menu::TerminalDeed;
    for deed in [
        Deed::Terminal(TerminalDeed::Clear),
        Deed::Action(Named::ClearTerminal),
        Deed::Action(Named::ClearRun),
        Deed::Action(Named::Rollback),
        Deed::Action(Named::DeleteFile),
    ] {
        assert!(loses(deed), "{deed:?} discards files or terminal history");
    }
    for deed in [
        Deed::Action(Named::StopRun),
        Deed::Action(Named::CloseAllTabs),
    ] {
        assert!(!loses(deed), "{deed:?} keeps files and output");
    }
}

#[test]
fn worktree_existing_strokes_only_describe_the_active_target() {
    use guild_desktop::actions::keymap::Named;
    use guild_desktop::view::menu::{Front, stroke_shown};
    for (deed, action) in [
        (WorktreeDeed::Run, Named::Run),
        (WorktreeDeed::Terminal, Named::NewTerminal),
        (WorktreeDeed::CopyPath, Named::CopyWorktreePath),
        (WorktreeDeed::Remove, Named::RemoveWorktree),
    ] {
        assert_eq!(
            stroke_shown(
                Deed::Column(deed),
                Front {
                    front: true,
                    divided: false
                }
            ),
            Some(action)
        );
        assert_eq!(stroke_shown(Deed::Column(deed), Front::default()), None);
    }
}

#[test]
fn folded_rollback_keeps_its_destructive_menu_classification() {
    use guild_desktop::actions::keymap::Named;
    use guild_desktop::view::menu::dock::dock_head;
    let offered = deeds(&dock_head(
        guild_session::Dock::Left,
        &[Named::Rollback, Named::RefreshChanges],
    ));
    assert!(offered.contains(&Deed::Action(Named::Rollback)));
    assert!(offered.contains(&Deed::Action(Named::RefreshChanges)));
    for deed in offered {
        assert_eq!(loses(deed), deed == Deed::Action(Named::Rollback));
    }
}

#[test]
fn root_copy_strokes_require_the_same_folder() {
    use guild_desktop::actions::keymap::Named;
    use guild_desktop::view::menu::{OpenInDeed, ProjectDeed, stroke_for};
    for deed in [
        Deed::Project(ProjectDeed::CopyPath),
        Deed::OpenIn(OpenInDeed::CopyPath),
    ] {
        assert_eq!(stroke_for(deed, true), Some(Named::CopyWorktreePath));
        assert_eq!(stroke_for(deed, false), None);
    }
}

// These exercise the production group builder used by entries_for_target and
// frozen opened menus, rather than only the historical row-facts adapter.
#[test]
fn actual_commit_group_menu_preserves_actions_and_refuses_mixed_applicability() {
    use guild_desktop::{
        changes::{Listed, Row, Section},
        i18n::{Language, Ui},
        theme::Mark as FileMark,
        view::menu::changes::entries_for_selection,
    };
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
    };
    let row = |path: &str, section, conflict| Listed {
        section,
        row: Row {
            path: path.into(),
            mark: FileMark::Modified,
            from: None,
            is_dir: false,
            unmerged: conflict,
            staged: false,
        },
    };
    let rows = vec![
        row("tracked", Section::Changes, false),
        row("new", Section::Unversioned, false),
        row("conflict", Section::Changes, true),
    ];
    let selected = vec![PathBuf::from("tracked")];
    let ticks = HashSet::from([PathBuf::from("tracked")]);
    let build = |paths: &[PathBuf]| {
        entries_for_selection(
            &rows,
            &HashMap::new(),
            paths,
            &ticks,
            &HashSet::new(),
            &HashSet::new(),
            Ui(Language::English),
            true,
        )
    };
    let single = build(&selected);
    let single_deeds = deeds(&single);
    for deed in [
        Deed::Change(ChangeDeed::ShowDiff),
        Deed::Change(ChangeDeed::ShowDiffStaged),
        Deed::Change(ChangeDeed::ShowDiffUnstaged),
        Deed::Path(PathDeed::Reveal),
        Deed::Change(ChangeDeed::Rollback),
        Deed::Change(ChangeDeed::Exclude),
        Deed::Change(ChangeDeed::Delete),
    ] {
        assert!(
            single_deeds.contains(&deed),
            "single selection preserves {deed:?}"
        );
    }
    assert!(single.contains(&Entry::Note(Label::Text(Text::CommitSpaceHint))));
    let mixed = build(&["tracked".into(), "new".into()]);
    let mixed_deeds = deeds(&mixed);
    for deed in [
        ChangeDeed::Include,
        ChangeDeed::Exclude,
        ChangeDeed::Delete,
        ChangeDeed::CopyPaths,
    ] {
        assert!(mixed_deeds.contains(&Deed::Change(deed)));
    }
    for deed in [ChangeDeed::AddToVcs, ChangeDeed::Rollback] {
        assert!(
            !mixed_deeds.contains(&Deed::Change(deed)),
            "mixed selection cannot silently skip an ineligible member"
        );
    }
    for text in [Text::CommitAddUnavailable, Text::CommitRollbackUnavailable] {
        assert!(
            mixed
                .iter()
                .any(|e| matches!(e,Entry::Choice(c) if c.label==Label::Text(text) && !c.enabled))
        );
    }
    let conflict = deeds(&build(&["conflict".into()]));
    for deed in [
        ChangeDeed::Include,
        ChangeDeed::Exclude,
        ChangeDeed::Rollback,
        ChangeDeed::AddToVcs,
    ] {
        assert!(!conflict.contains(&Deed::Change(deed)));
    }
    assert!(conflict.contains(&Deed::Path(PathDeed::Reveal)));
    let no_space = entries_for_selection(
        &rows,
        &HashMap::new(),
        &selected,
        &ticks,
        &HashSet::new(),
        &HashSet::new(),
        Ui(Language::English),
        false,
    );
    assert!(!no_space.contains(&Entry::Note(Label::Text(Text::CommitSpaceHint))));
}

#[test]
fn actual_commit_menu_localizes_counts_readiness_and_refusals() {
    use guild_desktop::{
        changes::{Directory, EligibilityError, Listed, Row, Section},
        i18n::{CommitLabel, Language, Ui},
        theme::Mark as FileMark,
        view::menu::changes::entries_for_selection,
    };
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
    };
    let ui = Ui(Language::Russian);
    for (n, item, path, file) in [
        (1, "1 элемент", "1 относительный путь", "1 файл"),
        (2, "2 элемента", "2 относительных пути", "2 файла"),
        (5, "5 элементов", "5 относительных путей", "5 файлов"),
        (11, "11 элементов", "11 относительных путей", "11 файлов"),
        (21, "21 элемент", "21 относительный путь", "21 файл"),
    ] {
        assert!(
            ui.commit_menu_label(CommitLabel::Include, n)
                .ends_with(item)
        );
        assert!(
            ui.commit_menu_label(CommitLabel::CopyPaths, n)
                .ends_with(path)
        );
        assert!(ui.commit_menu_label(CommitLabel::Add, n).ends_with(file));
        assert!(ui.commit_directory_count(n, n).starts_with(file));
        assert!(
            ui.trash_selection_detail(n)
                .contains("скрытые и игнорируемые файлы")
        );
    }
    let rows = vec![Listed {
        section: Section::Unversioned,
        row: Row {
            path: "dir".into(),
            mark: FileMark::Untracked,
            from: None,
            is_dir: true,
            unmerged: false,
            staged: false,
        },
    }];
    let selected = vec![PathBuf::from("dir")];
    let mut directories = HashMap::new();
    let loading = entries_for_selection(
        &rows,
        &directories,
        &selected,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        ui,
        true,
    );
    assert!(loading.iter().any(|e|matches!(e,Entry::Choice(c) if c.detail.as_ref().is_some_and(|detail|matches!(detail,Label::Named(s) if s.contains("ещё загружается"))))));
    directories.insert(
        "dir".into(),
        Ok(Directory {
            files: vec!["dir/file".into()],
            ..Default::default()
        }),
    );
    let ready = entries_for_selection(
        &rows,
        &directories,
        &selected,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        ui,
        true,
    );
    assert!(ready.iter().any(|e|matches!(e,Entry::NamedItem(s,Deed::Change(ChangeDeed::Include)) if s=="Включить в коммит: 1 элемент")));
    assert!(ready.contains(&Entry::Item(Text::MenuReveal, Deed::Path(PathDeed::Reveal))));
    for error in [
        EligibilityError::EmbeddedNul,
        EligibilityError::UnsupportedPath,
        EligibilityError::Missing("x".into()),
        EligibilityError::Conflict("x".into()),
        EligibilityError::Loading("x".into()),
        EligibilityError::Empty("x".into()),
        EligibilityError::Read("x".into(), "Invalid directory name".into()),
    ] {
        let text = ui.commit_eligibility(&error);
        assert!(
            !text.contains("eligible") && !text.contains("Invalid") && !text.contains("loading"),
            "localized refusal: {text}"
        );
    }
    for error in [
        "git failed: Path contains NUL",
        "git failed: Unsupported non-UTF-8 path",
        "git failed: Invalid directory name",
        "not a git repository",
        "git is not installed",
    ] {
        let localized = ui.commit_directory_error(error);
        assert!(
            !localized.contains("git failed")
                && !localized.contains("not a git")
                && !localized.contains("is not installed")
                && !localized.contains("Invalid")
                && !localized.contains("Unsupported"),
            "gateway guard: {localized}"
        );
    }
    let diagnostic = "fatal: invalid path 'Invalid directory name.txt'";
    assert!(
        ui.commit_directory_error(diagnostic).ends_with(diagnostic),
        "localizing guards must not rewrite upstream filenames or diagnostics"
    );
    assert!(ui.commit_ignored_only(21).contains("21 путь"));
    assert!(
        !Ui(Language::English)
            .commit_menu_label(CommitLabel::Include, 2)
            .contains("deferred")
    );
}

#[test]
fn production_commit_root_menu_explains_why_trash_is_disabled() {
    use guild_desktop::{
        changes::{Directory, Listed, Row, Section},
        i18n::{Language, Ui},
        theme::Mark as FileMark,
        view::menu::changes::entries_for_selection,
    };
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
    };
    let rows = vec![Listed {
        section: Section::Unversioned,
        row: Row {
            path: PathBuf::new(),
            mark: FileMark::Untracked,
            from: None,
            is_dir: true,
            unmerged: false,
            staged: false,
        },
    }];
    let dirs = HashMap::from([(
        PathBuf::new(),
        Ok(Directory {
            files: vec!["a.txt".into()],
            ..Default::default()
        }),
    )]);
    for language in [Language::English, Language::Russian] {
        let items = entries_for_selection(
            &rows,
            &dirs,
            &[PathBuf::new()],
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
            Ui(language),
            true,
        );
        assert!(
            !items
                .iter()
                .any(|e| matches!(e, Entry::NamedItem(_, Deed::Change(ChangeDeed::Delete))))
        );
        assert!(items.iter().any(|e| matches!(e, Entry::Choice(c) if !c.enabled && c.detail == Some(Label::Text(Text::CommitTrashRoot)))));
        assert!(
            items
                .iter()
                .any(|e| matches!(e, Entry::NamedItem(_, Deed::Change(ChangeDeed::Include))))
        );
        assert!(items.contains(&Entry::Item(
            Text::CommitExpandDirectory,
            Deed::Change(ChangeDeed::ToggleDirectory)
        )));
    }
}

#[test]
fn nul_refusal_is_localized_separately_from_utf8_refusal() {
    use guild_desktop::{
        changes::EligibilityError,
        i18n::{Language, Ui},
    };
    for language in [Language::English, Language::Russian] {
        let ui = Ui(language);
        let text = ui.commit_eligibility(&EligibilityError::EmbeddedNul);
        assert_eq!(text, ui.text(Text::CommitPathNul));
        assert!(text.contains("NUL") && !text.contains("UTF-8"));
        assert_ne!(
            text,
            ui.commit_eligibility(&EligibilityError::UnsupportedPath)
        );
        assert!(
            ui.commit_directory_error("git failed: Path contains NUL")
                .ends_with(&text)
        );
    }
}

/// Review r3 `bugs+impl-7`: a partial row is in the commit, so its menu
/// offers Exclude — and Include still, which takes it whole.
#[test]
fn a_partial_row_offers_exclude_and_include() {
    use guild_desktop::{
        changes::{Listed, Row, Section},
        i18n::{Language, Ui},
        theme::Mark as FileMark,
        view::menu::changes::entries_for_selection,
    };
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
    };
    let rows = vec![Listed {
        section: Section::Changes,
        row: Row {
            path: "partial".into(),
            mark: FileMark::Modified,
            from: None,
            is_dir: false,
            unmerged: false,
            staged: true,
        },
    }];
    let entries = entries_for_selection(
        &rows,
        &HashMap::new(),
        &[PathBuf::from("partial")],
        &HashSet::new(),
        &HashSet::from([PathBuf::from("partial")]),
        &HashSet::new(),
        Ui(Language::English),
        true,
    );
    let deeds = deeds(&entries);
    assert!(deeds.contains(&Deed::Change(ChangeDeed::Exclude)));
    assert!(deeds.contains(&Deed::Change(ChangeDeed::Include)));
}
