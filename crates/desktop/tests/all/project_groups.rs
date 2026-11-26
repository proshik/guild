//! Project groups in the column: which section a repository is drawn in,
//! in what order, and what a project dropped on a place means.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use guild_desktop::column::{
    Dragged, DropOn, DropRow, Item, Landing, Opened, Section, Zone, arrange, drop_zone, groups,
    items, landing, sort_groups,
};
use guild_session::ProjectId;
use guild_session::project_group::{ProjectGroup, ProjectGroupId};
use guild_session::project_sort::{Activity, Mode};

fn roots(paths: &[(&'static str, ProjectId)]) -> Vec<Opened<'static>> {
    paths
        .iter()
        .map(|&(path, project)| Opened {
            project,
            path: Path::new(path),
            name: Path::new(path).file_name().unwrap().to_str().unwrap(),
            repository: None,
        })
        .collect()
}

fn group(name: &str, members: &[&str], collapsed: bool) -> ProjectGroup {
    ProjectGroup {
        id: ProjectGroupId::new(),
        name: name.to_owned(),
        repositories: members.iter().map(PathBuf::from).collect(),
        collapsed,
        folder: None,
    }
}

fn keys(section: &Section) -> Vec<&Path> {
    section
        .repositories
        .iter()
        .map(|group| group.key.as_path())
        .collect()
}

#[test]
fn a_group_stands_where_its_first_member_comes_among_the_ungrouped() {
    let ids: Vec<_> = (0..5).map(|_| ProjectId::new()).collect();
    let found = groups(
        &roots(&[
            ("/a", ids[0]),
            ("/b", ids[1]),
            ("/c", ids[2]),
            ("/d", ids[3]),
            ("/e", ids[4]),
        ]),
        &HashMap::new(),
    );
    // Created second-then-first; members listed out of manual order.
    let later = group("Later", &["/d", "/b"], false);
    let earlier = group("Earlier", &["/c"], false);
    let saved = [later.clone(), earlier.clone()];
    let sections = arrange(found, &saved);
    assert_eq!(sections.len(), 4);
    assert!(sections[0].head.is_none());
    assert_eq!(keys(&sections[0]), [Path::new("/a")]);
    assert_eq!(sections[1].head.as_ref().unwrap().id, later.id);
    // A header knows how many of its group's projects are drawn under it.
    assert_eq!(sections[1].head.as_ref().unwrap().count, 2);
    assert_eq!(sections[2].head.as_ref().unwrap().count, 1);
    // Inside a group, the workspace's own order, not the group's list.
    assert_eq!(keys(&sections[1]), [Path::new("/b"), Path::new("/d")]);
    assert_eq!(sections[2].head.as_ref().unwrap().name, "Earlier");
    assert_eq!(keys(&sections[2]), [Path::new("/c")]);
    assert!(sections[3].head.is_none());
    assert_eq!(keys(&sections[3]), [Path::new("/e")]);
}

#[test]
fn under_a_sort_mode_a_group_stands_where_its_most_active_member_does() {
    let ids: Vec<_> = (0..4).map(|_| ProjectId::new()).collect();
    let mut found = groups(
        &roots(&[
            ("/a", ids[0]),
            ("/b", ids[1]),
            ("/c", ids[2]),
            ("/d", ids[3]),
        ]),
        &HashMap::new(),
    );
    // Later projects are busier; D, ungrouped, is the busiest of all.
    let busy = |row: &guild_desktop::column::Row| match row {
        guild_desktop::column::Row::Opened { project, .. } => {
            let at = ids.iter().position(|id| id == project).unwrap() as u64;
            Activity {
                last_turn: Some(at),
                ..Activity::default()
            }
        }
        _ => Activity::default(),
    };
    sort_groups(&mut found, Mode::RecentActivity, busy, |_| None);
    let saved = [
        group("First", &["/a", "/b"], false),
        group("Second", &["/c"], false),
    ];
    let sections = arrange(found, &saved);
    assert!(sections[0].head.is_none());
    assert_eq!(keys(&sections[0]), [Path::new("/d")]);
    assert_eq!(sections[1].head.as_ref().unwrap().name, "Second");
    // B is First's busiest: the group stands at B, and orders by the mode.
    assert_eq!(sections[2].head.as_ref().unwrap().name, "First");
    assert_eq!(keys(&sections[2]), [Path::new("/b"), Path::new("/a")]);
}

#[test]
fn running_agents_keep_their_projects_and_groups_on_top() {
    let ids: Vec<_> = (0..6).map(|_| ProjectId::new()).collect();
    let mut found = groups(
        &roots(&[
            ("/a", ids[0]),
            ("/b", ids[1]),
            ("/c", ids[2]),
            ("/d", ids[3]),
            ("/e", ids[4]),
            ("/f", ids[5]),
        ]),
        &HashMap::new(),
    );
    // Later projects are busier, so without running agents the column
    // would be in reverse order.
    let busy = |row: &guild_desktop::column::Row| match row {
        guild_desktop::column::Row::Opened { project, .. } => Activity {
            last_turn: Some(ids.iter().position(|id| id == project).unwrap() as u64),
            ..Activity::default()
        },
        _ => Activity::default(),
    };
    // A has had a running agent since 7 and C since 3; E, ungrouped,
    // since 5. They go to the top, the earlier to become active first.
    let active = |group: &guild_desktop::column::Group| match group.key.to_str() {
        Some("/a") => Some(7),
        Some("/c") => Some(3),
        Some("/e") => Some(5),
        _ => None,
    };
    sort_groups(&mut found, Mode::RecentActivity, busy, active);
    let saved = [
        group("Work", &["/a", "/b", "/c"], false),
        group("Play", &["/d"], false),
    ];
    let sections = arrange(found, &saved);
    assert_eq!(sections[0].head.as_ref().unwrap().name, "Work");
    assert_eq!(
        keys(&sections[0]),
        [Path::new("/c"), Path::new("/a"), Path::new("/b")]
    );
    // Consecutive ungrouped repositories share one unheaded section.
    assert!(sections[1].head.is_none());
    assert_eq!(keys(&sections[1]), [Path::new("/e"), Path::new("/f")]);
    assert_eq!(sections[2].head.as_ref().unwrap().name, "Play");
    assert_eq!(keys(&sections[2]), [Path::new("/d")]);
}

#[test]
fn a_group_with_nothing_to_show_draws_no_header() {
    let id = ProjectId::new();
    let found = groups(&roots(&[("/a", id)]), &HashMap::new());
    let sections = arrange(found, &[group("Gone", &["/elsewhere"], false)]);
    assert_eq!(sections.len(), 1);
    assert!(sections[0].head.is_none());
}

#[test]
fn without_groups_the_column_is_one_unheaded_section() {
    let ids: Vec<_> = (0..2).map(|_| ProjectId::new()).collect();
    let found = groups(&roots(&[("/a", ids[0]), ("/b", ids[1])]), &HashMap::new());
    let sections = arrange(found.clone(), &[]);
    assert_eq!(sections.len(), 1);
    assert!(sections[0].head.is_none());
    assert_eq!(sections[0].repositories, found);
}

#[test]
fn a_collapsed_group_draws_its_header_and_hides_its_repositories() {
    let ids: Vec<_> = (0..3).map(|_| ProjectId::new()).collect();
    let found = groups(
        &roots(&[("/a", ids[0]), ("/b", ids[1]), ("/c", ids[2])]),
        &HashMap::new(),
    );
    let saved = [group("Shut", &["/a"], true), group("Open", &["/b"], false)];
    let drawn = items(&arrange(found, &saved));
    let shape: Vec<String> = drawn
        .iter()
        .map(|item| match item {
            Item::Head(head) => format!("head {}", head.name),
            Item::Repository { group, .. } => group.key.display().to_string(),
        })
        .collect();
    assert_eq!(shape, ["head Shut", "head Open", "/b", "/c"]);
    // Each drawn repository says which group it is in.
    let open = saved[1].id;
    assert!(drawn.iter().any(|item| matches!(
        item,
        Item::Repository { group, section: Some(id) } if group.key == Path::new("/b") && *id == open
    )));
    assert!(drawn.iter().any(|item| matches!(
        item,
        Item::Repository { group, section: None } if group.key == Path::new("/c")
    )));
}

#[test]
fn a_drop_on_a_group_s_header_or_rows_moves_the_project_into_it() {
    let here = ProjectGroupId::new();
    let there = ProjectGroupId::new();
    assert_eq!(
        landing(None, &DropOn::Head(there)),
        Some(Landing {
            group: Some(Some(there)),
            reorder: false
        })
    );
    assert_eq!(
        landing(Some(here), &DropOn::Head(there)),
        Some(Landing {
            group: Some(Some(there)),
            reorder: false
        })
    );
    // Onto a repository of another group: into that group, at its place.
    assert_eq!(
        landing(
            Some(here),
            &DropOn::Repository {
                group: Some(there),
                itself: false
            }
        ),
        Some(Landing {
            group: Some(Some(there)),
            reorder: true
        })
    );
    // Its own group's header: nothing to do.
    assert_eq!(landing(Some(here), &DropOn::Head(here)), None);
}

#[test]
fn a_drop_among_the_ungrouped_takes_the_project_out_of_its_group() {
    let here = ProjectGroupId::new();
    assert_eq!(
        landing(
            Some(here),
            &DropOn::Repository {
                group: None,
                itself: false
            }
        ),
        Some(Landing {
            group: Some(None),
            reorder: true
        })
    );
    assert_eq!(
        landing(Some(here), &DropOn::Ungrouped),
        Some(Landing {
            group: Some(None),
            reorder: false
        })
    );
    assert_eq!(landing(None, &DropOn::Ungrouped), None);
}

#[test]
fn reordering_within_a_section_keeps_working() {
    let here = ProjectGroupId::new();
    for section in [None, Some(here)] {
        assert_eq!(
            landing(
                section,
                &DropOn::Repository {
                    group: section,
                    itself: false
                }
            ),
            Some(Landing {
                group: None,
                reorder: true
            })
        );
        assert_eq!(
            landing(
                section,
                &DropOn::Repository {
                    group: section,
                    itself: true
                }
            ),
            None
        );
    }
}

#[test]
fn move_to_group_lists_the_groups_marks_the_current_one_and_offers_a_new_one() {
    use guild_desktop::i18n::Text;
    use guild_desktop::view::menu::column::move_to_group_entries;
    use guild_desktop::view::menu::{Deed, Entry, ProjectDeed};
    let work = ProjectGroupId::new();
    let play = ProjectGroupId::new();
    let groups = [(work, "Work".to_owned()), (play, "Play".to_owned())];

    let entries = move_to_group_entries(&groups, Some(play));
    let Entry::Choice(first) = &entries[0] else {
        panic!("a group is a choice: {entries:?}")
    };
    assert_eq!(first.deed, Deed::Project(ProjectDeed::MoveToGroup(work)));
    assert!(first.enabled && first.check == Some(false));
    let Entry::Choice(current) = &entries[1] else {
        panic!("{entries:?}")
    };
    assert!(!current.enabled && current.check == Some(true));
    assert_eq!(entries[2], Entry::Separator);
    assert_eq!(
        entries[3..],
        [
            Entry::Item(
                Text::MenuNewProjectGroup,
                Deed::Project(ProjectDeed::NewGroup)
            ),
            Entry::Item(
                Text::MenuRemoveFromProjectGroup,
                Deed::Project(ProjectDeed::RemoveFromGroup)
            ),
        ]
    );

    // Ungrouped, and no groups yet: only New Group…, without a rule.
    assert_eq!(
        move_to_group_entries(&[], None),
        [Entry::Item(
            Text::MenuNewProjectGroup,
            Deed::Project(ProjectDeed::NewGroup)
        )]
    );
}

#[test]
fn a_group_header_offers_its_folder_s_deeds_then_rename_and_delete() {
    use guild_desktop::view::menu::column::{ProjectGroupDeed, entries_for_project_group};
    use guild_desktop::view::menu::{Deed, Entry};
    for bound in [false, true] {
        let deeds: Vec<_> = entries_for_project_group(bound)
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Item(_, deed) => Some(deed),
                _ => None,
            })
            .collect();
        assert_eq!(
            deeds,
            [
                Deed::ProjectGroup(ProjectGroupDeed::NewSession),
                Deed::ProjectGroup(ProjectGroupDeed::BindFolder),
                Deed::ProjectGroup(ProjectGroupDeed::Rename),
                Deed::ProjectGroup(ProjectGroupDeed::Delete)
            ]
        );
    }
}

/// Where on a row a drag is let go of: the top and bottom edges of a row
/// are the places between items, where a line is drawn; the middle of a
/// group's header takes a project into the group, and the middle of an
/// ungrouped project offers to merge the two into a group. The last
/// project of a group's bottom edge is the place after the whole group.
#[test]
fn a_row_s_edges_are_places_between_items_and_its_middle_is_the_row() {
    let (top, bottom) = (100.0, 128.0);
    let at = |row, dragged, y| drop_zone(row, dragged, top, bottom, y);
    let project = Dragged::Project;
    // An ungrouped project's block: before, itself, after.
    assert_eq!(at(DropRow::Repository, project, 101.0), Some(Zone::Before));
    assert_eq!(at(DropRow::Repository, project, 114.0), Some(Zone::Middle));
    assert_eq!(at(DropRow::Repository, project, 127.0), Some(Zone::After));
    // An open group's header: its bottom edge sits on its first project, so
    // everything below the top edge takes the project into the group.
    let open = DropRow::Head { collapsed: false };
    assert_eq!(at(open, project, 101.0), Some(Zone::Before));
    assert_eq!(at(open, project, 114.0), Some(Zone::Middle));
    assert_eq!(at(open, project, 127.0), Some(Zone::Middle));
    // A collapsed group's header has a place after it too.
    let shut = DropRow::Head { collapsed: true };
    assert_eq!(at(shut, project, 101.0), Some(Zone::Before));
    assert_eq!(at(shut, project, 114.0), Some(Zone::Middle));
    assert_eq!(at(shut, project, 127.0), Some(Zone::After));
    // Inside a group the places are lines too — between its projects, into
    // the group there; a project's middle there is no target. The bottom
    // edge of the group's last project is the place after the group.
    let inner = DropRow::Member { last: false };
    let last = DropRow::Member { last: true };
    assert_eq!(at(inner, project, 101.0), Some(Zone::Before));
    assert_eq!(at(inner, project, 114.0), None);
    assert_eq!(at(inner, project, 127.0), Some(Zone::After));
    assert_eq!(at(last, project, 114.0), None);
    assert_eq!(at(last, project, 127.0), Some(Zone::After));
    // The edge is the same few points at every height of row.
    assert_eq!(
        drop_zone(DropRow::Repository, project, 0.0, 200.0, 190.0),
        Some(Zone::Middle)
    );
    assert_eq!(
        drop_zone(DropRow::Repository, project, 0.0, 200.0, 196.0),
        Some(Zone::After)
    );
}

/// A dragged group never goes into anything: every top-level row is split
/// into the place before it and the place after it, an open group's header
/// is the place before that group, and inside a group only the last
/// project's bottom edge is a place — after the group.
#[test]
fn a_dragged_group_lands_only_between_top_level_items() {
    let (top, bottom) = (100.0, 128.0);
    let at = |row, y| drop_zone(row, Dragged::Group, top, bottom, y);
    assert_eq!(at(DropRow::Repository, 113.0), Some(Zone::Before));
    assert_eq!(at(DropRow::Repository, 115.0), Some(Zone::After));
    assert_eq!(
        at(DropRow::Head { collapsed: true }, 110.0),
        Some(Zone::Before)
    );
    assert_eq!(
        at(DropRow::Head { collapsed: true }, 120.0),
        Some(Zone::After)
    );
    assert_eq!(
        at(DropRow::Head { collapsed: false }, 125.0),
        Some(Zone::Before)
    );
    assert_eq!(at(DropRow::Member { last: false }, 127.0), None);
    assert_eq!(at(DropRow::Member { last: true }, 114.0), None);
    assert_eq!(at(DropRow::Member { last: true }, 127.0), Some(Zone::After));
}

/// GLD-055 decision 10: the deepest open root owns a path, whatever order
/// the desk holds the roots in.
#[test]
fn the_deepest_open_root_owns_a_path() {
    use guild_desktop::column::owning_root;
    let group = ProjectId::new();
    let backend = ProjectId::new();
    let worktree = ProjectId::new();
    // The group's folder first, as `desk.projects()` can hold it.
    let roots = [
        (group, Path::new("/w/acme")),
        (backend, Path::new("/w/acme/backend")),
        (worktree, Path::new("/w/acme/backend/.worktrees/fix")),
    ];
    assert_eq!(
        owning_root(roots, Path::new("/w/acme/backend/src/main.rs")),
        Some((backend, PathBuf::from("src/main.rs")))
    );
    assert_eq!(
        owning_root(roots, Path::new("/w/acme/AGENTS.md")),
        Some((group, PathBuf::from("AGENTS.md")))
    );
    // A linked worktree nested in its main checkout owns its own files.
    assert_eq!(
        owning_root(roots, Path::new("/w/acme/backend/.worktrees/fix/a.rs")),
        Some((worktree, PathBuf::from("a.rs")))
    );
    assert_eq!(owning_root(roots, Path::new("/elsewhere/x")), None);
}

/// GLD-055 decision 15: the header's menu offers a session in the folder and
/// the folder's binding before Rename and Delete; without a folder the
/// session asks for one, and *Bind* stands where *Change* would.
#[test]
fn the_group_menu_names_the_folder_s_deeds_with_and_without_one() {
    use guild_desktop::i18n::Text;
    use guild_desktop::view::menu::Entry;
    use guild_desktop::view::menu::column::entries_for_project_group;
    let labels = |bound| -> Vec<Option<Text>> {
        entries_for_project_group(bound)
            .into_iter()
            .map(|entry| match entry {
                Entry::Item(text, _) => Some(text),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        labels(true),
        [
            Some(Text::MenuGroupNewSession),
            Some(Text::MenuGroupChangeFolder),
            None,
            Some(Text::MenuRenameProjectGroup),
            None,
            Some(Text::MenuDeleteProjectGroup),
        ]
    );
    assert_eq!(
        labels(false),
        [
            Some(Text::MenuGroupNewSessionAsk),
            Some(Text::MenuGroupBindFolder),
            None,
            Some(Text::MenuRenameProjectGroup),
            None,
            Some(Text::MenuDeleteProjectGroup),
        ]
    );
}

/// Decision 16: each project the `+` offers wears its tile, and a member of
/// another group says which.
#[test]
fn the_header_plus_offers_each_project_with_its_tile_and_its_group() {
    use guild_desktop::view::menu::column::{AddChoice, add_project_entries};
    use guild_desktop::view::menu::{Entry, Label, Mark};
    let entries = add_project_entries(
        &[
            AddChoice {
                key: PathBuf::from("/w/guild"),
                name: "guild".to_owned(),
                group: None,
            },
            AddChoice {
                key: PathBuf::from("/w/atlas"),
                name: "atlas".to_owned(),
                group: Some("agent-review".to_owned()),
            },
        ],
        |group| format!("in {group}"),
    );
    let rows: Vec<_> = entries
        .into_iter()
        .map(|entry| match entry {
            Entry::Choice(choice) => (choice.mark, choice.detail),
            _ => panic!("a choice"),
        })
        .collect();
    assert_eq!(
        rows,
        [
            (Mark::Tile("guild".to_owned()), None),
            (
                Mark::Tile("atlas".to_owned()),
                Some(Label::Named("in agent-review".to_owned()))
            ),
        ]
    );
}
