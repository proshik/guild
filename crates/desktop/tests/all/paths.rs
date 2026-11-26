//! Where the window looks for things, and what it hands a session — M13.

use std::ffi::OsStr;
use std::path::Path;

use guild_desktop::paths::path_with;

/// **`guild` goes in front of a session's `PATH`, and only once.** In
/// front, so a session finds this window's `guild` rather than another
/// build of it; once, because a session a session started inherits a path
/// that already carries it, and a path that grows by one entry a
/// generation is one nobody can read.
#[test]
fn the_command_s_folder_goes_first_and_is_never_there_twice() {
    let dir = Path::new("/Applications/Guild.app/Contents/MacOS");

    let fresh = path_with(dir, Some(OsStr::new("/usr/bin:/bin")));
    assert_eq!(
        fresh,
        "/Applications/Guild.app/Contents/MacOS:/usr/bin:/bin"
    );

    // A child of a session already has it — somewhere, even not first.
    let inherited = path_with(
        dir,
        Some(OsStr::new(
            "/usr/bin:/Applications/Guild.app/Contents/MacOS:/bin",
        )),
    );
    assert_eq!(
        inherited,
        "/Applications/Guild.app/Contents/MacOS:/usr/bin:/bin"
    );

    // No path at all is the folder alone, not an empty entry after it.
    assert_eq!(
        path_with(dir, None),
        "/Applications/Guild.app/Contents/MacOS"
    );
}
