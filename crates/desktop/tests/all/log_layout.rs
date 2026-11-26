//! How the Log lays itself out: three columns when there is the width,
//! the stacked shape otherwise.

use guild_desktop::log_layout::{COMMITS_MIN, LogLayout, columns, layout};
use guild_session::Columns;

/// Three columns need width: the bottom dock has it, a side dock does
/// not.
#[test]
fn the_log_is_wide_from_seven_hundred() {
    assert_eq!(layout(1512.0), LogLayout::Wide);
    assert_eq!(layout(700.0), LogLayout::Wide);
    assert_eq!(layout(699.0), LogLayout::Narrow);
    assert_eq!(layout(230.0), LogLayout::Narrow);
}

/// The outer columns are what the hand dragged them to, while the panel
/// has the room.
#[test]
fn the_outer_columns_are_what_the_hand_left_them_at() {
    let wanted = Columns {
        branches: 260.0,
        details: 380.0,
        message: Columns::MESSAGE,
        push_files: Columns::PUSH_FILES,
    };
    assert_eq!(columns(1400.0, wanted), (260.0, 380.0));
    assert_eq!(
        columns(900.0, Columns::default()),
        (Columns::BRANCHES, Columns::DETAILS)
    );
}

/// When the room runs out the outer columns give way, and the commits
/// keep their minimum: the list is what the other two are about.
#[test]
fn the_outer_columns_give_way_before_the_commits_do() {
    let greedy = Columns {
        branches: 400.0,
        details: 640.0,
        message: Columns::MESSAGE,
        push_files: Columns::PUSH_FILES,
    };
    let (branches, details) = columns(700.0, greedy);
    assert!(branches >= Columns::BRANCHES_BOUNDS.0);
    assert!(details >= Columns::DETAILS_BOUNDS.0);
    assert!(
        700.0 - branches - details >= COMMITS_MIN,
        "the commits kept {COMMITS_MIN}, got {}",
        700.0 - branches - details
    );
}

/// At the narrowest wide panel the branches are what gives way: they
/// are short names, while the details hold a body and a file list and
/// stop being readable at all when squeezed.
#[test]
fn the_branches_give_way_before_the_details() {
    let (branches, details) = columns(700.0, Columns::default());
    assert_eq!(details, Columns::DETAILS, "the details keep their width");
    assert_eq!(branches, 700.0 - COMMITS_MIN - Columns::DETAILS);
    assert!(branches < Columns::BRANCHES);
}
