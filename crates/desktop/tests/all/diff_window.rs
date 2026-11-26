//! The diff window's stepping — M8.3.6, decision 7.

use guild_desktop::diff_window::stepped;

#[test]
fn stepping_through_files_stops_at_both_ends() {
    assert_eq!(stepped(0, 5, true), Some(1));
    assert_eq!(stepped(3, 5, true), Some(4));
    assert_eq!(stepped(4, 5, true), None, "the last file");
    assert_eq!(stepped(4, 5, false), Some(3));
    assert_eq!(stepped(0, 5, false), None, "the first file");
    assert_eq!(stepped(0, 1, true), None, "one file goes nowhere");
    assert_eq!(stepped(0, 0, false), None);
}
