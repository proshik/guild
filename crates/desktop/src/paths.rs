//! Where the desktop keeps its state.
//!
//! The path is computed here rather than inside `guild-session`: the
//! session crate deliberately takes a ready path so that it can be swapped
//! in tests. The decision "where exactly" is an application concern and
//! lives with the application.

use std::path::{Path, PathBuf};

/// The state directory: `$GUILD_STATE_DIR` when it is set, otherwise
/// `$XDG_CONFIG_HOME/guild`, otherwise `~/.config/guild`.
///
/// `GUILD_STATE_DIR` is for a second window on one machine — the build
/// being developed beside the Guild being worked in. It is not
/// `XDG_CONFIG_HOME` for a reason: every session the window starts
/// inherits the environment, and git, gh and opencode read their own
/// configuration from under that variable, so moving it would move theirs.
///
/// If the home directory is unknown too, we put it next to the current
/// directory. That is worse, but better than failing to start: the
/// application works without settings, but not without a window.
pub fn state_dir() -> PathBuf {
    located(
        std::env::var("GUILD_STATE_DIR").ok(),
        std::env::var("XDG_CONFIG_HOME").ok(),
        std::env::var("HOME").ok(),
    )
}

/// The same decision on explicit values, so that a test need not set the
/// process's environment.
fn located(guild: Option<String>, xdg: Option<String>, home: Option<String>) -> PathBuf {
    if let Some(dir) = guild.filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(xdg) = xdg.filter(|xdg| !xdg.is_empty()) {
        return PathBuf::from(xdg).join("guild");
    }
    match home {
        Some(home) if !home.is_empty() => PathBuf::from(home).join(".config").join("guild"),
        _ => PathBuf::from(".guild"),
    }
}

/// The settings file.
pub fn settings_file() -> PathBuf {
    state_dir().join("settings.json")
}

/// The state file: projects and their sessions.
///
/// The name differs from the previous `sessions.json` deliberately: the
/// format changed, and the old file is read and migrated. Writing new
/// content under the old name would take away the option to roll back.
pub fn workspace_file() -> PathBuf {
    state_dir().join("workspace.json")
}

/// The keymap file.
pub fn keymap_file() -> PathBuf {
    state_dir().join("keymap.json")
}

/// The state file in the previous format — a flat list of sessions with
/// no projects.
pub fn legacy_sessions_file() -> PathBuf {
    state_dir().join("sessions.json")
}

/// Where `claude` keeps its session transcripts.
///
/// Its directory, not ours: it writes the files, we only read them. That
/// is where we learn whether a terminal session has a conversation at all.
pub fn claude_projects_dir() -> PathBuf {
    // The rule lives in the session crate since M13, where the `guild`
    // command can reach it too.
    guild_session::transcript::projects_dir()
}

/// The directory worth offering as the first project.
///
/// `None` means there is nothing to offer. That is what happens with a
/// bundled application: started from Finder, it gets the root as its
/// working directory, and without this check the first run would create a
/// project named `/`.
///
/// The root and the home directory are both rejected: neither is ever a
/// project, and walking their contents takes a long time.
pub fn plausible_first_project() -> Option<PathBuf> {
    offerable(std::env::current_dir().ok()?, std::env::var("HOME").ok())
}

/// The same decision on explicit values.
///
/// Split out so that it can be checked without touching the process's
/// working directory: `set_current_dir` is process-wide, cargo's tests run
/// in threads, and a test that moves the process out from under its
/// neighbours fails them at random.
fn offerable(cwd: PathBuf, home: Option<String>) -> Option<PathBuf> {
    if cwd == Path::new("/") {
        return None;
    }
    if let Some(home) = home
        && !home.is_empty()
        && cwd == Path::new(&home)
    {
        return None;
    }

    Some(cwd)
}

/// The folder the `guild` command ships in, when it ships beside this
/// window — M13.
///
/// `target/debug` while developing, `Guild.app/Contents/MacOS` once
/// bundled: the two binaries are built and packed side by side, so the
/// window finds its own command by looking next to itself. `None` when
/// there is no `guild` there, and then nothing is added to anyone's path.
pub fn guild_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    dir.join("guild").is_file().then(|| dir.to_path_buf())
}

/// A `PATH` with `dir` in front of `existing`, and not in it twice — M13.
///
/// **In front**, because a session told to run `guild` must find this
/// window's `guild` rather than another build of it somewhere on the path;
/// and **once**, because a session a session started inherits the path
/// already carrying it, and a path that grows by one entry per generation
/// is a path nobody can read.
pub fn path_with(dir: &Path, existing: Option<&std::ffi::OsStr>) -> std::ffi::OsString {
    let mut parts: Vec<PathBuf> = vec![dir.to_path_buf()];
    if let Some(existing) = existing {
        parts.extend(std::env::split_paths(existing).filter(|part| part != dir));
    }
    std::env::join_paths(parts).unwrap_or_else(|_| dir.as_os_str().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second window — the one being developed beside the one being
    /// worked in — keeps its state apart by `GUILD_STATE_DIR`, and that
    /// wins over everything else.
    #[test]
    fn the_state_directory_named_outright_wins() {
        let dir = located(
            Some("/tmp/dev".into()),
            Some("/x".into()),
            Some("/h".into()),
        );

        assert_eq!(dir, PathBuf::from("/tmp/dev"));
    }

    #[test]
    fn otherwise_the_state_directory_is_xdg_then_home() {
        let xdg = located(Some(String::new()), Some("/x".into()), Some("/h".into()));
        let home = located(None, None, Some("/h".into()));

        assert_eq!(xdg, PathBuf::from("/x/guild"));
        assert_eq!(home, PathBuf::from("/h/.config/guild"));
    }

    /// A bundled application's working directory is the root. A project
    /// named `/` is not how you greet someone on their first run.
    #[test]
    fn the_root_directory_is_not_offered_as_a_project() {
        let offered = offerable(PathBuf::from("/"), Some(String::from("/Users/someone")));

        assert_eq!(offered, None, "the root is never a project");
    }

    /// The home directory is not a project either: it is never one, and
    /// walking its contents takes a long time.
    #[test]
    fn the_home_directory_is_not_offered_as_a_project() {
        let home = String::from("/Users/someone");
        let offered = offerable(PathBuf::from(&home), Some(home.clone()));

        assert_eq!(offered, None, "the home directory is never a project");
    }

    #[test]
    fn an_ordinary_directory_is_offered() {
        let cwd = PathBuf::from("/Users/someone/work/guild");
        let offered = offerable(cwd.clone(), Some(String::from("/Users/someone")));

        assert_eq!(offered, Some(cwd));
    }

    /// An unset HOME is not a reason to refuse: without it there is simply
    /// nothing to compare against.
    #[test]
    fn a_missing_home_does_not_block_the_offer() {
        let cwd = PathBuf::from("/srv/guild");

        assert_eq!(offerable(cwd.clone(), None), Some(cwd.clone()));
        assert_eq!(offerable(cwd.clone(), Some(String::new())), Some(cwd));
    }
}
