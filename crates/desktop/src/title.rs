//! What a tab is called.
//!
//! The program says. A terminal program sets its window title through OSC,
//! and both of ours do it well: `claude` writes `✳ Claude Code` at first
//! and then the conversation's topic — `✳ Rust uninstall workflow` — while
//! `zsh` writes `user@host:/path/to/project`. That is what terminals show
//! on their tabs, and it is what a person recognises their session by.
//!
//! Neither goes on a tab as it arrives. The marker is decoration, the
//! `user@host` half is the same on every tab a person has, and a title has
//! no length limit while a tab strip does.

/// How much of a title fits on a tab.
///
/// Long enough for a topic — `Rust uninstall workflow` is 23 — and short
/// enough that four tabs still fit across the strip.
pub const LIMIT: usize = 28;

/// The tab's name when a person may have given it one — M12.1.
///
/// **What a person typed wins** over whatever the tab would call itself —
/// `automatic`, the topic or the program's title — and is cut to the same
/// length, because the strip is the same strip. A blank name is no name.
pub fn chosen(name: Option<&str>, automatic: Option<String>) -> Option<String> {
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => Some(truncate(name, LIMIT)),
        None => automatic,
    }
}

/// The tab's name, from the title the program set.
///
/// `None` when there is nothing worth showing yet, and the caller falls
/// back to what it knows: a project's name, or the kind of tab.
pub fn of(raw: &str) -> Option<String> {
    let trimmed = strip_marker(raw.trim());
    if trimmed.is_empty() {
        return None;
    }

    let shortened = shorten_path(trimmed);
    Some(truncate(shortened, LIMIT))
}

/// Drop a leading decorative marker.
///
/// `claude` prefixes its title with `✳`. On a tab it is noise repeated on
/// every Claude tab a person has; the words after it are what tells them
/// apart.
fn strip_marker(title: &str) -> &str {
    title
        .trim_start_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '~')
        .trim()
}

/// Turn `user@host:/path/to/project` into `project`.
///
/// A shell writes the whole of it, and the `user@host` half is identical
/// on every tab. What differs — and what a person is looking for — is the
/// last part of the path.
fn shorten_path(title: &str) -> &str {
    let path = match title.split_once(':') {
        Some((before, after)) if before.contains('@') => after,
        _ => title,
    };

    if !path.starts_with('/') && !path.starts_with('~') {
        return title;
    }

    path.trim_end_matches('/')
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(path)
}

/// Cut a long title, marking that it was cut.
///
/// By characters rather than bytes: cutting a multi-byte character in half
/// panics, and a topic can be written in any alphabet.
pub(crate) fn truncate(title: &str, limit: usize) -> String {
    if title.chars().count() <= limit {
        return title.to_owned();
    }

    let kept: String = title.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}
