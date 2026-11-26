use std::path::{Path, PathBuf};

use guild_session::{Desk, WorkspaceId};

fn desk_with(paths: &[&str]) -> (Desk, WorkspaceId) {
    let mut desk = Desk::default();
    desk.normalise();
    let workspace = desk.active_workspace();
    for path in paths {
        desk.open_in(workspace, Path::new(path));
    }
    (desk, workspace)
}

fn members(desk: &Desk, workspace: WorkspaceId) -> Vec<(String, Vec<PathBuf>)> {
    desk.workspace(workspace)
        .unwrap()
        .project_groups
        .iter()
        .map(|group| (group.name.clone(), group.repositories.clone()))
        .collect()
}

#[test]
fn a_group_is_made_with_its_first_repository_and_a_blank_name_is_refused() {
    let (mut desk, workspace) = desk_with(&["/a", "/b"]);
    assert_eq!(
        desk.create_project_group(workspace, "   ", Path::new("/a")),
        None
    );
    // A repository the workspace does not hold cannot found a group.
    assert_eq!(
        desk.create_project_group(workspace, "Work", Path::new("/elsewhere")),
        None
    );
    let id = desk
        .create_project_group(workspace, "  Work ", Path::new("/a"))
        .unwrap();
    assert_eq!(
        members(&desk, workspace),
        [("Work".to_owned(), vec![PathBuf::from("/a")])]
    );
    assert_eq!(
        desk.workspace(workspace)
            .unwrap()
            .project_group_of(Path::new("/a"))
            .map(|group| group.id),
        Some(id)
    );
}

#[test]
fn the_last_repository_leaving_a_group_takes_the_group_with_it() {
    let (mut desk, workspace) = desk_with(&["/a", "/b", "/c"]);
    let first = desk
        .create_project_group(workspace, "First", Path::new("/a"))
        .unwrap();
    let second = desk
        .create_project_group(workspace, "Second", Path::new("/b"))
        .unwrap();
    assert!(desk.move_to_project_group(workspace, Path::new("/c"), Some(first)));
    // Moving its only member to another group empties and removes Second.
    assert!(desk.move_to_project_group(workspace, Path::new("/b"), Some(first)));
    assert!(
        desk.workspace(workspace)
            .unwrap()
            .project_group(second)
            .is_none()
    );
    // Taken out of the group, one by one: the group lasts until the last.
    assert!(desk.move_to_project_group(workspace, Path::new("/a"), None));
    assert!(desk.move_to_project_group(workspace, Path::new("/b"), None));
    assert_eq!(
        members(&desk, workspace),
        [("First".to_owned(), vec![PathBuf::from("/c")])]
    );
    assert!(desk.move_to_project_group(workspace, Path::new("/c"), None));
    assert!(desk.workspace(workspace).unwrap().project_groups.is_empty());
    // Nothing to change answers false, so the window does not save.
    assert!(!desk.move_to_project_group(workspace, Path::new("/c"), None));
}

#[test]
fn a_new_group_takes_its_repository_out_of_the_old_one() {
    let (mut desk, workspace) = desk_with(&["/a"]);
    desk.create_project_group(workspace, "Old", Path::new("/a"))
        .unwrap();
    desk.create_project_group(workspace, "New", Path::new("/a"))
        .unwrap();
    assert_eq!(
        members(&desk, workspace),
        [("New".to_owned(), vec![PathBuf::from("/a")])]
    );
}

#[test]
fn removing_a_repository_from_the_workspace_leaves_its_group() {
    let (mut desk, workspace) = desk_with(&["/a", "/b"]);
    let id = desk
        .create_project_group(workspace, "Pair", Path::new("/a"))
        .unwrap();
    desk.move_to_project_group(workspace, Path::new("/b"), Some(id));
    let a = desk.projects_of(workspace).next().unwrap().id;
    desk.leave(workspace, a);
    assert_eq!(
        members(&desk, workspace),
        [("Pair".to_owned(), vec![PathBuf::from("/b")])]
    );
    let b = desk.projects_of(workspace).next().unwrap().id;
    desk.leave(workspace, b);
    assert!(desk.workspace(workspace).unwrap().project_groups.is_empty());
}

#[test]
fn a_repository_stays_grouped_while_another_of_its_worktrees_is_open() {
    let (mut desk, workspace) = desk_with(&["/repo", "/repo-feature"]);
    let common = PathBuf::from("/repo/.git");
    let ids: Vec<_> = desk.projects_of(workspace).map(|p| p.id).collect();
    for id in &ids {
        desk.set_repository(*id, Some(common.clone()));
    }
    desk.create_project_group(workspace, "Repo", &common)
        .unwrap();
    desk.leave(workspace, ids[1]);
    assert_eq!(
        members(&desk, workspace),
        [("Repo".to_owned(), vec![common])]
    );
}

#[test]
fn moving_a_repository_to_another_workspace_takes_it_out_of_its_group() {
    let (mut desk, workspace) = desk_with(&["/a", "/b"]);
    let other = desk.add_workspace("Other", None).unwrap();
    let id = desk
        .create_project_group(workspace, "Only", Path::new("/a"))
        .unwrap();
    let a = desk
        .projects_of(workspace)
        .find(|p| p.path == Path::new("/a"))
        .unwrap()
        .id;
    assert!(desk.move_projects(workspace, other, &[a]));
    assert!(
        desk.workspace(workspace)
            .unwrap()
            .project_group(id)
            .is_none()
    );
    // It arrives ungrouped: groups are not shared between workspaces.
    assert!(desk.workspace(other).unwrap().project_groups.is_empty());
}

#[test]
fn deleting_a_group_returns_its_repositories_and_closes_nothing() {
    let (mut desk, workspace) = desk_with(&["/a", "/b"]);
    let id = desk
        .create_project_group(workspace, "Both", Path::new("/a"))
        .unwrap();
    desk.move_to_project_group(workspace, Path::new("/b"), Some(id));
    let before: Vec<_> = desk.projects_of(workspace).map(|p| p.id).collect();
    assert!(desk.delete_project_group(workspace, id));
    assert!(desk.workspace(workspace).unwrap().project_groups.is_empty());
    let after: Vec<_> = desk.projects_of(workspace).map(|p| p.id).collect();
    assert_eq!(before, after);
    assert!(!desk.delete_project_group(workspace, id));
}

#[test]
fn rename_trims_and_refuses_blank() {
    let (mut desk, workspace) = desk_with(&["/a"]);
    let id = desk
        .create_project_group(workspace, "Work", Path::new("/a"))
        .unwrap();
    assert!(!desk.rename_project_group(workspace, id, "  "));
    assert!(desk.rename_project_group(workspace, id, " Play "));
    assert_eq!(members(&desk, workspace)[0].0, "Play");
}

#[test]
fn groups_are_one_workspace_s_own() {
    let (mut desk, first) = desk_with(&["/a"]);
    let second = desk.add_workspace("Second", None).unwrap();
    desk.open_in(second, Path::new("/a"));
    let id = desk
        .create_project_group(first, "Mine", Path::new("/a"))
        .unwrap();
    assert!(desk.workspace(second).unwrap().project_groups.is_empty());
    // Another workspace cannot reach the group by its id.
    assert!(!desk.move_to_project_group(second, Path::new("/a"), Some(id)));
    assert!(!desk.rename_project_group(second, id, "Theirs"));
}

#[test]
fn collapse_and_groups_persist_and_a_desk_without_groups_writes_none() {
    let (mut desk, workspace) = desk_with(&["/a", "/b"]);
    let plain = serde_json::to_value(&desk).unwrap();
    assert!(plain["workspaces"][0].get("project_groups").is_none());

    let id = desk
        .create_project_group(workspace, "Work", Path::new("/a"))
        .unwrap();
    assert!(desk.toggle_project_group(workspace, id));
    let json = serde_json::to_string(&desk).unwrap();
    let mut restored: Desk = serde_json::from_str(&json).unwrap();
    restored.normalise();
    let group = restored
        .workspace(workspace)
        .unwrap()
        .project_group(id)
        .unwrap();
    assert!(group.collapsed);
    assert_eq!(group.repositories, [PathBuf::from("/a")]);

    // An expanded group does not write the flag.
    assert!(!restored.toggle_project_group(workspace, id));
    let value = serde_json::to_value(&restored).unwrap();
    assert!(
        value["workspaces"][0]["project_groups"][0]
            .get("collapsed")
            .is_none()
    );
}

#[test]
fn workspace_json_written_before_groups_loads_unchanged() {
    let desk: Desk = serde_json::from_str(
        r#"{
        "projects": [
          {"id":"00000000-0000-0000-0000-000000000001","path":"/one","name":"one"}
        ],
        "workspaces": [
          {"id":"00000000-0000-0000-0000-0000000000aa","name":"Default",
           "projects":["00000000-0000-0000-0000-000000000001"]}
        ]
    }"#,
    )
    .unwrap();
    assert!(desk.workspaces()[0].project_groups.is_empty());
    let saved = serde_json::to_value(&desk).unwrap();
    assert!(saved["workspaces"][0].get("project_groups").is_none());
}

#[test]
fn selecting_a_row_expands_its_collapsed_group() {
    let (mut desk, workspace) = desk_with(&["/a"]);
    let id = desk
        .create_project_group(workspace, "Work", Path::new("/a"))
        .unwrap();
    desk.toggle_project_group(workspace, id);
    assert!(desk.expand_project_group_of(workspace, Path::new("/a")));
    assert!(
        !desk
            .workspace(workspace)
            .unwrap()
            .project_group(id)
            .unwrap()
            .collapsed
    );
    assert!(!desk.expand_project_group_of(workspace, Path::new("/a")));
}

#[test]
fn a_root_whose_repository_git_names_later_keeps_its_group() {
    let (mut desk, workspace) = desk_with(&["/repo"]);
    let id = desk
        .create_project_group(workspace, "Work", Path::new("/repo"))
        .unwrap();
    let root = desk.projects_of(workspace).next().unwrap().id;
    desk.set_repository(root, Some(PathBuf::from("/repo/.git")));
    assert_eq!(
        desk.workspace(workspace)
            .unwrap()
            .project_group(id)
            .unwrap()
            .repositories,
        [PathBuf::from("/repo/.git")]
    );
}

fn order(desk: &Desk, workspace: WorkspaceId) -> Vec<PathBuf> {
    desk.workspace(workspace)
        .unwrap()
        .projects
        .iter()
        .map(|id| desk.project(*id).unwrap().path.clone())
        .collect()
}

/// State written while groups were drawn above the ungrouped projects in
/// creation order loads looking the same: the manual order is put into
/// that shape once, and the workspace says so, so a later move sticks.
#[test]
fn a_workspace_saved_with_groups_on_top_keeps_that_look_once_groups_share_the_order() {
    let (mut desk, workspace) = desk_with(&["/a", "/b", "/c", "/d", "/e"]);
    let later = desk
        .create_project_group(workspace, "Later", Path::new("/d"))
        .unwrap();
    desk.move_to_project_group(workspace, Path::new("/b"), Some(later));
    desk.create_project_group(workspace, "Earlier", Path::new("/c"));
    let mut value = serde_json::to_value(&desk).unwrap();
    // What an older Guild wrote: the roots in the order they were opened,
    // and no word about where the groups stand.
    let ids: Vec<_> = ["/a", "/b", "/c", "/d", "/e"]
        .iter()
        .map(|path| {
            desk.projects()
                .find(|p| p.path == Path::new(path))
                .unwrap()
                .id
                .to_string()
        })
        .collect();
    value["workspaces"][0]["projects"] = serde_json::json!(ids);
    value["workspaces"][0]
        .as_object_mut()
        .unwrap()
        .remove("groups_in_order");
    let mut restored: Desk = serde_json::from_value(value).unwrap();
    restored.normalise();
    // Later's members, Earlier's, then the ungrouped — each in its order.
    assert_eq!(
        order(&restored, workspace),
        ["/b", "/d", "/c", "/a", "/e"].map(PathBuf::from)
    );
    assert!(restored.workspace(workspace).unwrap().groups_in_order);
    let saved = serde_json::to_value(&restored).unwrap();
    assert_eq!(saved["workspaces"][0]["groups_in_order"], true);

    // Once in order, a move sticks: put /a above both groups, save, load.
    let _ = saved;
    let a = restored
        .projects()
        .find(|p| p.path == Path::new("/a"))
        .unwrap()
        .id;
    let b = restored
        .projects()
        .find(|p| p.path == Path::new("/b"))
        .unwrap()
        .id;
    assert!(restored.reorder_projects_at(workspace, &[a], b, false));
    let mut again: Desk = serde_json::from_value(serde_json::to_value(&restored).unwrap()).unwrap();
    again.normalise();
    assert_eq!(
        order(&again, workspace),
        ["/a", "/b", "/d", "/c", "/e"].map(PathBuf::from)
    );
}

/// A group's roots stay together however a root arrives: a worktree opened
/// later, whose repository git names afterwards, joins its group's block,
/// so closing the group's first root does not move the group.
#[test]
fn a_worktree_opened_later_joins_its_group_s_block() {
    let (mut desk, workspace) = desk_with(&["/repo", "/x"]);
    let common = PathBuf::from("/repo/.git");
    let first = desk
        .projects()
        .find(|p| p.path == Path::new("/repo"))
        .unwrap()
        .id;
    desk.set_repository(first, Some(common.clone()));
    desk.create_project_group(workspace, "Work", &common)
        .unwrap();
    let linked = desk.open_in(workspace, Path::new("/repo-feature"));
    desk.set_repository(linked, Some(common.clone()));
    assert_eq!(
        order(&desk, workspace),
        ["/repo", "/repo-feature", "/x"].map(PathBuf::from)
    );
    desk.leave(workspace, first);
    assert_eq!(
        order(&desk, workspace),
        ["/repo-feature", "/x"].map(PathBuf::from),
        "the group stays above /x"
    );
}

#[test]
fn a_workspace_without_groups_writes_no_order_flag() {
    let (desk, _) = desk_with(&["/a"]);
    let value = serde_json::to_value(&desk).unwrap();
    assert!(value["workspaces"][0].get("groups_in_order").is_none());
}

/// Joining a group puts the repository beside the group's members, so the
/// group — which stands where its first member does — does not jump to
/// where the newcomer was.
#[test]
fn a_repository_joining_a_group_moves_beside_its_members() {
    let (mut desk, workspace) = desk_with(&["/a", "/b", "/c", "/d"]);
    let id = desk
        .create_project_group(workspace, "Work", Path::new("/c"))
        .unwrap();
    assert!(desk.move_to_project_group(workspace, Path::new("/a"), Some(id)));
    assert_eq!(
        order(&desk, workspace),
        ["/b", "/c", "/a", "/d"].map(PathBuf::from)
    );
}

/// A group made from a repository stands where that repository stood.
#[test]
fn a_new_group_stands_where_its_repository_was() {
    let (mut desk, workspace) = desk_with(&["/a", "/b", "/c"]);
    desk.create_project_group(workspace, "Work", Path::new("/b"));
    assert_eq!(
        order(&desk, workspace),
        ["/a", "/b", "/c"].map(PathBuf::from)
    );
    assert!(desk.workspace(workspace).unwrap().groups_in_order);
}

/// Every top-level item keeps its roots in one block while a workspace has
/// groups: an ungrouped repository's second checkout, opened after another
/// project, moves up beside its first, so a group dropped after it lands
/// right after it.
#[test]
fn an_ungrouped_repository_s_checkouts_stay_together_beside_groups() {
    let (mut desk, workspace) = desk_with(&["/h", "/b", "/c"]);
    desk.create_project_group(workspace, "Here", Path::new("/h"))
        .unwrap();
    let common = PathBuf::from("/b/.git");
    let b = desk
        .projects()
        .find(|p| p.path == Path::new("/b"))
        .unwrap()
        .id;
    desk.set_repository(b, Some(common.clone()));
    let linked = desk.open_in(workspace, Path::new("/b-feature"));
    desk.set_repository(linked, Some(common));
    assert_eq!(
        order(&desk, workspace),
        ["/h", "/b", "/b-feature", "/c"].map(PathBuf::from)
    );
}

/// A group made from a repository whose checkouts are apart gathers them.
#[test]
fn a_new_group_gathers_its_repository_s_checkouts() {
    let (mut desk, workspace) = desk_with(&["/repo", "/x"]);
    let common = PathBuf::from("/repo/.git");
    let first = desk
        .projects()
        .find(|p| p.path == Path::new("/repo"))
        .unwrap()
        .id;
    desk.set_repository(first, Some(common.clone()));
    let linked = desk.open_in(workspace, Path::new("/repo-feature"));
    desk.set_repository(linked, Some(common.clone()));
    assert_eq!(
        order(&desk, workspace),
        ["/repo", "/x", "/repo-feature"].map(PathBuf::from),
        "without groups the order is left as it was"
    );
    desk.create_project_group(workspace, "Work", &common)
        .unwrap();
    assert_eq!(
        order(&desk, workspace),
        ["/repo", "/repo-feature", "/x"].map(PathBuf::from)
    );
}

/// A checkout moved into a workspace whose group holds its repository
/// joins the group's block there.
#[test]
fn a_checkout_moved_into_a_workspace_joins_its_group_s_block() {
    let (mut desk, first) = desk_with(&["/repo-feature"]);
    let common = PathBuf::from("/repo/.git");
    let linked = desk
        .projects()
        .find(|p| p.path == Path::new("/repo-feature"))
        .unwrap()
        .id;
    desk.set_repository(linked, Some(common.clone()));
    let second = desk.add_workspace("Other", None).unwrap();
    let main = desk.open_in(second, Path::new("/repo"));
    desk.set_repository(main, Some(common.clone()));
    desk.open_in(second, Path::new("/x"));
    desk.create_project_group(second, "Work", &common).unwrap();
    assert!(desk.move_projects(first, second, &[linked]));
    assert_eq!(
        order(&desk, second),
        ["/repo", "/repo-feature", "/x"].map(PathBuf::from)
    );
}

// GLD-055: a group bound to a folder.

use guild_session::project_group::{FolderRefused, main_checkout, suggest_folder};

fn same(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[test]
fn a_bound_folder_is_kept_and_an_old_state_file_without_it_still_loads() {
    let (mut desk, workspace) = desk_with(&["/w/acme/backend", "/w/acme/frontend"]);
    let id = desk
        .create_project_group(workspace, "acme", Path::new("/w/acme/backend"))
        .unwrap();
    // An old file: a group written before groups had folders.
    let old = serde_json::to_value(desk.workspace(workspace).unwrap()).unwrap();
    assert!(old["project_groups"][0].get("folder").is_none());
    let loaded: guild_session::workspace::Workspace = serde_json::from_value(old).unwrap();
    assert_eq!(loaded.project_group(id).unwrap().folder, None);

    let root = desk
        .bind_project_group_folder(workspace, id, Path::new("/w/acme"))
        .unwrap();
    assert_eq!(desk.project(root).unwrap().path, Path::new("/w/acme"));
    let saved = serde_json::to_string(&desk).unwrap();
    let back: Desk = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        back.workspace(workspace)
            .unwrap()
            .project_group(id)
            .unwrap()
            .folder
            .as_deref(),
        Some(Path::new("/w/acme"))
    );
    assert_eq!(back.project_group_root(root), Some((workspace, id)));
}

#[test]
fn the_suggestion_is_the_one_shared_parent_and_never_home_or_the_filesystem_root() {
    let paths = |all: &[&str]| all.iter().map(PathBuf::from).collect::<Vec<_>>();
    let home = Path::new("/Users/p");
    assert_eq!(
        suggest_folder(
            &paths(&["/Users/p/acme/backend", "/Users/p/acme/helm"]),
            Some(home)
        ),
        Some(PathBuf::from("/Users/p/acme"))
    );
    // Different parents: nothing.
    assert_eq!(
        suggest_folder(
            &paths(&["/Users/p/acme/backend", "/Users/p/other/helm"]),
            Some(home)
        ),
        None
    );
    // The home directory: nothing.
    assert_eq!(
        suggest_folder(&paths(&["/Users/p/backend", "/Users/p/helm"]), Some(home)),
        None
    );
    // The filesystem root: nothing.
    assert_eq!(
        suggest_folder(&paths(&["/backend", "/helm"]), Some(home)),
        None
    );
    // No members: nothing.
    assert_eq!(suggest_folder(&[], Some(home)), None);
}

#[test]
fn the_folder_root_is_held_apart_from_the_members_and_does_not_keep_the_group_alive() {
    let (mut desk, workspace) = desk_with(&["/w/acme/backend", "/w/acme/frontend"]);
    let id = desk
        .create_project_group(workspace, "acme", Path::new("/w/acme/backend"))
        .unwrap();
    desk.move_to_project_group(workspace, Path::new("/w/acme/frontend"), Some(id));
    let root = desk
        .bind_project_group_folder(workspace, id, Path::new("/w/acme"))
        .unwrap();
    assert!(desk.is_project_group_root(root));
    // Not a member of its own group.
    assert_eq!(
        members(&desk, workspace),
        [(
            "acme".to_owned(),
            vec![
                PathBuf::from("/w/acme/backend"),
                PathBuf::from("/w/acme/frontend")
            ]
        )]
    );
    assert!(desk.take_released_group_folders().is_empty());

    // The members leave; the folder root is still open, yet the group goes,
    // and its folder is handed to the window to close.
    desk.move_to_project_group(workspace, Path::new("/w/acme/backend"), None);
    desk.move_to_project_group(workspace, Path::new("/w/acme/frontend"), None);
    assert!(members(&desk, workspace).is_empty());
    assert_eq!(
        desk.take_released_group_folders(),
        [(workspace, PathBuf::from("/w/acme"))]
    );
    assert!(desk.take_released_group_folders().is_empty());
    assert!(!desk.is_project_group_root(root));
}

#[test]
fn a_member_closing_last_releases_the_folder_as_well() {
    let (mut desk, workspace) = desk_with(&["/w/acme/backend"]);
    let id = desk
        .create_project_group(workspace, "acme", Path::new("/w/acme/backend"))
        .unwrap();
    desk.bind_project_group_folder(workspace, id, Path::new("/w/acme"))
        .unwrap();
    let backend = desk
        .projects_of(workspace)
        .find(|project| project.path == Path::new("/w/acme/backend"))
        .unwrap()
        .id;
    desk.leave(workspace, backend);
    assert!(members(&desk, workspace).is_empty());
    assert_eq!(
        desk.take_released_group_folders(),
        [(workspace, PathBuf::from("/w/acme"))]
    );
}

#[test]
fn a_folder_that_is_a_members_root_or_another_groups_folder_is_refused() {
    let (mut desk, workspace) = desk_with(&["/w/acme/backend", "/w/other/helm"]);
    let acme = desk
        .create_project_group(workspace, "acme", Path::new("/w/acme/backend"))
        .unwrap();
    let other = desk
        .create_project_group(workspace, "other", Path::new("/w/other/helm"))
        .unwrap();
    assert_eq!(
        desk.bind_project_group_folder(workspace, acme, Path::new("/w/acme/backend")),
        Err(FolderRefused::OwnRoot)
    );
    desk.bind_project_group_folder(workspace, other, Path::new("/w/shared"))
        .unwrap();
    assert_eq!(
        desk.bind_project_group_folder(workspace, acme, Path::new("/w/shared")),
        Err(FolderRefused::OtherGroup)
    );
    let gone = guild_session::project_group::ProjectGroupId::new();
    assert_eq!(
        desk.bind_project_group_folder(workspace, gone, Path::new("/w/x")),
        Err(FolderRefused::NoGroup)
    );
}

#[test]
fn renaming_a_group_keeps_its_folder() {
    let (mut desk, workspace) = desk_with(&["/w/acme/backend"]);
    let id = desk
        .create_project_group(workspace, "acme", Path::new("/w/acme/backend"))
        .unwrap();
    desk.bind_project_group_folder(workspace, id, Path::new("/w/acme"))
        .unwrap();
    assert!(desk.rename_project_group(workspace, id, "Acme system"));
    let group = desk
        .workspace(workspace)
        .unwrap()
        .project_group(id)
        .unwrap();
    assert_eq!(group.folder.as_deref(), Some(Path::new("/w/acme")));
}

#[test]
fn the_match_says_which_members_are_direct_children_of_the_folder() {
    let (mut desk, workspace) = desk_with(&[
        "/w/acme/backend",
        "/w/acme/frontend",
        "/w/elsewhere/helm",
        "/w/acme/deeper/tools",
    ]);
    let id = desk
        .create_project_group(workspace, "acme", Path::new("/w/acme/backend"))
        .unwrap();
    for key in [
        "/w/acme/frontend",
        "/w/elsewhere/helm",
        "/w/acme/deeper/tools",
    ] {
        desk.move_to_project_group(workspace, Path::new(key), Some(id));
    }
    let holder = desk.workspace(workspace).unwrap();
    assert_eq!(holder.project_group_match(id, main_checkout, same), None);
    desk.bind_project_group_folder(workspace, id, Path::new("/w/acme"))
        .unwrap();
    let found = desk
        .workspace(workspace)
        .unwrap()
        .project_group_match(id, main_checkout, same)
        .unwrap();
    let inside: Vec<_> = found
        .members
        .iter()
        .map(|member| (member.checkout.clone(), member.inside))
        .collect();
    assert_eq!(
        inside,
        [
            (PathBuf::from("/w/acme/backend"), true),
            (PathBuf::from("/w/acme/frontend"), true),
            (PathBuf::from("/w/elsewhere/helm"), false),
            // A nested deeper path is not a direct child.
            (PathBuf::from("/w/acme/deeper/tools"), false),
        ]
    );
    assert!(!found.matches());
    assert_eq!(found.inside(), 2);
}

#[test]
fn a_git_members_checkout_is_the_folder_its_git_directory_sits_in() {
    use guild_session::project_group::main_checkout;
    assert_eq!(
        main_checkout(Path::new("/w/acme/backend/.git")),
        PathBuf::from("/w/acme/backend")
    );
    assert_eq!(
        main_checkout(Path::new("/w/plain")),
        PathBuf::from("/w/plain")
    );
}

#[test]
fn a_symlinked_folder_matches_what_it_points_to() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("acme");
    std::fs::create_dir_all(real.join("backend")).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let backend = real.join("backend");
    let mut desk = Desk::default();
    desk.normalise();
    let workspace = desk.active_workspace();
    desk.open_in(workspace, &backend);
    let id = desk
        .create_project_group(workspace, "acme", &backend)
        .unwrap();
    desk.bind_project_group_folder(workspace, id, &link)
        .unwrap();
    let resolve = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let holder = desk.workspace(workspace).unwrap();
    assert!(
        !holder
            .project_group_match(id, main_checkout, same)
            .unwrap()
            .members[0]
            .inside
    );
    assert!(
        holder
            .project_group_match(id, main_checkout, resolve)
            .unwrap()
            .matches()
    );
}

#[test]
fn a_member_root_of_any_group_in_any_workspace_cannot_be_a_folder() {
    let (mut desk, first) = desk_with(&["/w/acme/backend", "/w/other/helm"]);
    let second = desk.add_workspace("Second", None).unwrap();
    desk.open_in(second, Path::new("/w/other/helm"));
    desk.create_project_group(first, "first", Path::new("/w/acme/backend"))
        .unwrap();
    // helm is a member of a group in the second workspace…
    desk.create_project_group(second, "ops", Path::new("/w/other/helm"))
        .unwrap();
    let acme = desk
        .workspace(first)
        .unwrap()
        .project_groups
        .first()
        .unwrap()
        .id;
    // …so it is no group's folder, in this workspace or that one.
    assert_eq!(
        desk.bind_project_group_folder(first, acme, Path::new("/w/other/helm")),
        Err(FolderRefused::OwnRoot)
    );
}

#[test]
fn the_folder_s_own_repository_is_never_admitted_and_never_keeps_a_group_alive() {
    let (mut desk, workspace) = desk_with(&["/w/acme/backend", "/w/acme-tools"]);
    let id = desk
        .create_project_group(workspace, "acme", Path::new("/w/acme/backend"))
        .unwrap();
    let folder = desk
        .bind_project_group_folder(workspace, id, Path::new("/w/acme"))
        .unwrap();
    // The folder is a git repository of its own, and another of its
    // checkouts is open as an ordinary root.
    desk.set_repository(folder, Some(PathBuf::from("/w/acme/.git")));
    let tools = desk
        .projects_of(workspace)
        .find(|project| project.path == Path::new("/w/acme-tools"))
        .unwrap()
        .id;
    desk.set_repository(tools, Some(PathBuf::from("/w/acme/.git")));
    assert!(!desk.move_to_project_group(workspace, Path::new("/w/acme/.git"), Some(id)));
    assert_eq!(
        desk.create_project_group(workspace, "tools", Path::new("/w/acme/.git")),
        None
    );
}

#[test]
fn a_folder_bound_in_one_workspace_is_a_group_root_in_every_other_and_never_a_member() {
    let (mut desk, first) = desk_with(&["/w/acme/backend"]);
    let second = desk.add_workspace("Second", None).unwrap();
    desk.open_in(second, Path::new("/w/other/helm"));
    let id = desk
        .create_project_group(first, "acme", Path::new("/w/acme/backend"))
        .unwrap();
    let folder = desk
        .bind_project_group_folder(first, id, Path::new("/w/acme"))
        .unwrap();
    // Held by the second workspace as an ordinary root too…
    desk.open_in(second, Path::new("/w/acme"));
    assert!(desk.is_project_group_root(folder));
    // …where it founds no group and joins none.
    assert_eq!(
        desk.create_project_group(second, "x", Path::new("/w/acme")),
        None
    );
    let ops = desk
        .create_project_group(second, "ops", Path::new("/w/other/helm"))
        .unwrap();
    assert!(!desk.move_to_project_group(second, Path::new("/w/acme"), Some(ops)));
    // Closed in the first workspace, the binding still holds the role.
    desk.leave(first, folder);
    assert!(desk.is_project_group_root(folder));
}
