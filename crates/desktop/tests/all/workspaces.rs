use std::path::Path;

use guild_desktop::view::workspaces::grid_step;
use guild_desktop::workspaces::{Meant, stepped, workspace_for};
use guild_session::WorkspaceId;

fn id(n: u8) -> WorkspaceId {
    format!("00000000-0000-0000-0000-{n:012}").parse().unwrap()
}

#[test]
fn workspace_steps_wrap_both_ways_and_one_stays_put() {
    assert_eq!(stepped(3, 0, false), 2);
    assert_eq!(stepped(3, 2, true), 0);
    assert_eq!(stepped(1, 0, true), 0);
    assert_eq!(stepped(1, 0, false), 0);
}

/// **The repository's workspace before the caller's** — M13.1, amending
/// decision 17 of M12.5. The report it came from is the third row: an
/// agent in *Side* started a session in a worktree of a repository only
/// *Acme* holds, and the tab opened in *Side*, where the person was not
/// looking.
#[test]
fn command_workspace_resolution_has_one_fixed_precedence() {
    let (front, asked, caller, other) = (id(1), id(2), id(3), id(4));
    let names = vec![
        (front, "Default".into()),
        (caller, "Side".into()),
        (other, "Acme".into()),
    ];
    let path = Path::new("/w/atlas");

    // `--workspace` over everything.
    assert_eq!(
        workspace_for(Some(asked), Some(caller), &[other], front, &names, path),
        Meant::Workspace(asked)
    );
    // One holder, whoever asks — and whether anyone does.
    assert_eq!(
        workspace_for(None, Some(caller), &[other], front, &names, path),
        Meant::Workspace(other),
        "the repository's only workspace wins over the caller's"
    );
    assert_eq!(
        workspace_for(None, None, &[other], front, &names, path),
        Meant::Workspace(other)
    );
    // Several holders: the caller's when it is one of them.
    assert_eq!(
        workspace_for(None, Some(caller), &[caller, other], front, &names, path),
        Meant::Workspace(caller)
    );
    // Several, and the caller is not among them — or there is no caller,
    // even when the front one is among them: refused, naming them.
    for caller in [Some(caller), None] {
        let Meant::Refused(because) =
            workspace_for(None, caller, &[front, other], front, &names, path)
        else {
            panic!("ambiguous root was accepted for caller {caller:?}")
        };
        assert!(
            because.contains("Default, Acme") && because.contains("--workspace"),
            "{because}"
        );
    }
    // No holder: the caller's, otherwise the front one.
    assert_eq!(
        workspace_for(None, Some(caller), &[], front, &names, path),
        Meant::Workspace(caller)
    );
    assert_eq!(
        workspace_for(None, None, &[], front, &names, path),
        Meant::Workspace(front)
    );
}

/// The icon picker's addressable space: 0 is *No icon*, above the grid,
/// and 1.. are its cells, `columns` to a row — 3 rows of 3 here:
/// ```text
/// [No icon]
///  1  2  3
///  4  5  6
///  7  8  9
/// ```
#[test]
fn grid_step_moves_left_and_right_inside_a_row_without_wrapping() {
    // The middle of the first row moves either way…
    assert_eq!(grid_step(10, 3, Some(2), "right", None), Some(3));
    assert_eq!(grid_step(10, 3, Some(2), "left", None), Some(1));
    // …and its ends do not wrap into the row beside them.
    assert_eq!(grid_step(10, 3, Some(1), "left", None), Some(1));
    assert_eq!(grid_step(10, 3, Some(3), "right", None), Some(3));
}

#[test]
fn grid_step_moves_up_and_down_by_a_row_without_wrapping() {
    // Cell 5 is the middle of the middle row; a row up or down lands on
    // the cell directly above or below it.
    assert_eq!(grid_step(10, 3, Some(5), "up", None), Some(2));
    assert_eq!(grid_step(10, 3, Some(5), "down", None), Some(8));
    // The last row has nothing below it, so `down` stays put — this is
    // "no wrap", not "clamp to the nearest cell of a shorter row".
    assert_eq!(grid_step(10, 3, Some(8), "down", None), Some(8));
}

#[test]
fn grid_step_reaches_no_icon_above_the_first_row_and_leaves_it_by_down_only() {
    // `up` from the first row reaches the *No icon* row above the grid.
    assert_eq!(grid_step(10, 3, Some(2), "up", None), Some(0));
    // From *No icon*, only `down` moves — onto the grid's first cell.
    assert_eq!(grid_step(10, 3, Some(0), "down", None), Some(1));
    assert_eq!(grid_step(10, 3, Some(0), "up", None), Some(0));
    assert_eq!(grid_step(10, 3, Some(0), "left", None), Some(0));
    assert_eq!(grid_step(10, 3, Some(0), "right", None), Some(0));
}

#[test]
fn grid_step_from_nothing_goes_to_the_current_icon_or_the_first() {
    // Nothing lit yet: the first key just reveals where the selection
    // already is, rather than also moving it.
    assert_eq!(grid_step(10, 3, None, "down", Some(5)), Some(5));
    // No current icon either: the first row, *No icon*.
    assert_eq!(grid_step(10, 3, None, "left", None), Some(0));
}
