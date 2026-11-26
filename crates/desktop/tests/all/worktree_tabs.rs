//! A worktree's tabs listed under its row in the column: which tabs, and
//! in what order.

use guild_desktop::column::{TabFact, worktree_tabs};
use guild_session::{AreaId, ProjectId, Seat, WorkspaceId};

fn seat(workspace: WorkspaceId, project: ProjectId) -> Seat {
    Seat { workspace, project }
}

#[test]
fn only_this_seat_s_session_tabs_are_listed_in_strip_order() {
    let workspace = WorkspaceId::new();
    let here = seat(workspace, ProjectId::new());
    let other = seat(workspace, ProjectId::new());
    let elsewhere = seat(WorkspaceId::new(), here.project);
    let area = AreaId::new();
    let tabs = [
        TabFact {
            id: 1,
            seat: here,
            area,
            session: true,
        },
        // A file, a diff or Settings holds no session: not listed.
        TabFact {
            id: 2,
            seat: here,
            area,
            session: false,
        },
        TabFact {
            id: 3,
            seat: other,
            area,
            session: true,
        },
        // The same root held by another workspace is another seat.
        TabFact {
            id: 4,
            seat: elsewhere,
            area,
            session: true,
        },
        TabFact {
            id: 5,
            seat: here,
            area,
            session: true,
        },
    ];
    assert_eq!(worktree_tabs(&tabs, here, &[area]), [1, 5]);
    assert_eq!(worktree_tabs(&tabs, other, &[area]), [3]);
}

#[test]
fn a_divided_seat_lists_its_areas_in_order_each_in_strip_order() {
    let here = seat(WorkspaceId::new(), ProjectId::new());
    let (left, right) = (AreaId::new(), AreaId::new());
    let tabs = [
        TabFact {
            id: 'a',
            seat: here,
            area: right,
            session: true,
        },
        TabFact {
            id: 'b',
            seat: here,
            area: left,
            session: true,
        },
        TabFact {
            id: 'c',
            seat: here,
            area: right,
            session: true,
        },
        TabFact {
            id: 'd',
            seat: here,
            area: left,
            session: true,
        },
    ];
    assert_eq!(
        worktree_tabs(&tabs, here, &[left, right]),
        ['b', 'd', 'a', 'c']
    );
    // An area the stage has not told about keeps the window's order, last.
    assert_eq!(worktree_tabs(&tabs, here, &[left]), ['b', 'd', 'a', 'c']);
    assert_eq!(worktree_tabs(&tabs, here, &[]), ['a', 'b', 'c', 'd']);
}

#[test]
fn a_worktree_without_session_tabs_lists_nothing() {
    let here = seat(WorkspaceId::new(), ProjectId::new());
    let area = AreaId::new();
    let tabs = [TabFact {
        id: 1,
        seat: here,
        area,
        session: false,
    }];
    assert!(worktree_tabs(&tabs, here, &[area]).is_empty());
}
