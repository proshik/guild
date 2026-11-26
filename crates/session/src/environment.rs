//! The environment `claude` is launched with.
//!
//! Guild starts **its own** session. If Guild itself was started from a
//! Claude Code session — which happens often, agents work on this very
//! code — the environment carries markers of that other session, and the
//! child `claude` takes them for its own.
//!
//! It shows up immediately: `⚠ Transcript saving is off — inherited
//! CLAUDE_CODE_CHILD_SESSION marker`. The session still works but writes
//! no transcript, and our "a conversation happened" signal rests on that
//! transcript: without it a terminal session looks empty, and resuming it
//! will never be offered.

/// Parent-session markers that must not leak into the child.
///
/// The list is by name rather than by the `CLAUDE_` prefix: the person's
/// own settings live under that prefix too, and wiping them along the way
/// would change `claude`'s behaviour behind their back. Only what
/// describes **someone else's session** is removed.
const PARENT_SESSION_MARKERS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
];

/// Does this variable describe someone else's session?
pub fn is_parent_session_marker(name: &str) -> bool {
    PARENT_SESSION_MARKERS.contains(&name)
}

/// What has to be removed from the environment before launching.
///
/// Returns names rather than a ready environment: `tokio::process::Command`
/// and `portable_pty::CommandBuilder` set it in different ways, while the
/// decision about what to remove is the same for both.
pub fn markers_to_drop() -> impl Iterator<Item = &'static str> {
    PARENT_SESSION_MARKERS.iter().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The marker named in the warning itself. This is where it started.
    #[test]
    fn the_child_session_marker_is_dropped() {
        assert!(is_parent_session_marker("CLAUDE_CODE_CHILD_SESSION"));
    }

    #[test]
    fn markers_of_the_parent_session_are_dropped() {
        for name in [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_PID",
        ] {
            assert!(is_parent_session_marker(name), "{name} must be removed");
        }
    }

    /// The person's own settings live under the same prefix. Wiping them
    /// along the way would change `claude`'s behaviour behind their back —
    /// which is why the list is by name, not by prefix.
    #[test]
    fn settings_of_the_person_are_left_alone() {
        for name in [
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_API_KEY",
            "ANTHROPIC_API_KEY",
            "PATH",
            "HOME",
            "TERM",
        ] {
            assert!(
                !is_parent_session_marker(name),
                "{name} must be left alone: it is not a session marker"
            );
        }
    }

    #[test]
    fn the_list_is_not_empty_and_has_no_duplicates() {
        let mut names: Vec<&str> = markers_to_drop().collect();
        assert!(!names.is_empty());

        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(count, names.len(), "the list contains duplicates");
    }
}
