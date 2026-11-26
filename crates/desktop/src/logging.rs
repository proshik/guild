//! Where the application's messages go.
//!
//! An application started from Finder has no visible `stderr`: everything
//! we write — "session cannot be restored", "tab failed to open" — goes
//! nowhere. There is then nothing to work from.
//!
//! So the log always goes to a file, and additionally to the terminal when
//! the application was started from one.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing_subscriber::EnvFilter;

/// Where to put the log.
///
/// `~/Library/Logs/Guild` is the place the system sets aside for it:
/// Console.app looks there, and so does a person who has been asked to
/// send their logs.
pub fn directory() -> PathBuf {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => PathBuf::from(home).join("Library/Logs/Guild"),
        _ => PathBuf::from(".guild-logs"),
    }
}

/// The log file.
pub fn file() -> PathBuf {
    directory().join("guild.log")
}

/// Start logging.
///
/// Failures to open the file do not bring the application down: having no
/// log is bad, but failing to start because of it is worse.
pub fn start() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    match open(&file()) {
        Some(handle) => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                // No colour: colouring in a file turns into a mess of
                // escape sequences.
                .with_ansi(false)
                .with_writer(Mutex::new(handle))
                .init();
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                log = %file().display(),
                "Guild started"
            );
        }
        None => {
            tracing_subscriber::fmt().with_env_filter(filter).init();
            tracing::warn!(
                path = %file().display(),
                "could not open the log file, writing to the terminal"
            );
        }
    }
}

/// Send panics to the log.
///
/// An application started from Finder has no visible `stderr`, which is
/// why this file exists — and a panic is exactly the moment it has to
/// work, because there is no window left to ask. Without this the window
/// vanishes and the log's last line is whatever happened before it.
///
/// The previous hook runs afterwards rather than being replaced: in a
/// terminal run it is what still prints to `stderr`, and taking that away
/// would make development worse in order to make Finder better.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        let place = info.location().map(std::panic::Location::to_string);
        tracing::error!(
            backtrace = %std::backtrace::Backtrace::force_capture(),
            "{}",
            describe(&payload(info), place.as_deref())
        );
        previous(info);
    }));
}

/// The message a panic carries.
///
/// A payload is `&str` or `String` in practice and anything at all in
/// principle. An unknown one still gets a line: that something panicked,
/// and where, is most of the answer.
fn payload(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();

    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic with no message".to_owned()
    }
}

/// One line saying what happened and where.
fn describe(message: &str, place: Option<&str>) -> String {
    match place {
        Some(place) => format!("panic at {place}: {message}"),
        None => format!("panic in an unknown place: {message}"),
    }
}

fn open(path: &Path) -> Option<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }

    // Appending rather than truncating: the previous run's log is what
    // gets read after a crash.
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;

    trim(path);
    Some(file)
}

/// Keep the log from growing without limit.
///
/// Terminal sessions are verbose, and a file nobody trims eventually takes
/// gigabytes. We cut the beginning rather than the end: recent entries
/// matter more than old ones.
fn trim(path: &Path) {
    const LIMIT: u64 = 8 * 1024 * 1024;

    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() <= LIMIT {
        return;
    }

    let Ok(text) = std::fs::read_to_string(path) else {
        // Not text means corrupted; start over.
        let _ = std::fs::write(path, b"");
        return;
    };

    let keep: String = text
        .lines()
        .skip(text.lines().count() / 2)
        .map(|line| format!("{line}\n"))
        .collect();
    let _ = std::fs::write(path, keep);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_lives_where_the_system_looks_for_it() {
        let path = file();

        assert!(
            path.to_string_lossy().contains("Library/Logs/Guild"),
            "the log must live where Console.app looks for it: {}",
            path.display()
        );
        assert!(path.ends_with("guild.log"));
    }

    /// A log nobody trims eventually takes gigabytes: terminal sessions
    /// are verbose.
    #[test]
    fn an_oversized_log_is_trimmed_keeping_the_recent_half() {
        let folder = tempfile::Builder::new()
            .prefix("guild-log-")
            .tempdir()
            .expect("a scratch folder");
        let path = folder.path().join("guild.log");

        let line = "x".repeat(1000);
        let text: String = (0..20_000).map(|i| format!("{i}-{line}\n")).collect();
        std::fs::write(&path, &text).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 8 * 1024 * 1024);

        trim(&path);

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.len() < text.len(), "the file must shrink");
        assert!(
            after.contains("19999-"),
            "recent entries must remain — those are the ones read"
        );
        assert!(
            !after.contains("\n0-"),
            "old ones are cut: they matter less"
        );
    }

    #[test]
    fn a_small_log_is_left_alone() {
        let folder = tempfile::Builder::new()
            .prefix("guild-log-small-")
            .tempdir()
            .expect("a scratch folder");
        let path = folder.path().join("guild.log");
        std::fs::write(&path, "one line\n").unwrap();

        trim(&path);

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one line\n");
    }

    /// What the log has to contain to be worth reading afterwards: what
    /// happened, and where.
    #[test]
    fn a_panic_is_described_by_its_message_and_place() {
        let described = describe(
            "the grid mutex is not reentrant",
            Some("src/terminal/view.rs:412"),
        );

        assert!(described.contains("the grid mutex is not reentrant"));
        assert!(described.contains("src/terminal/view.rs:412"));
    }

    /// A panic can carry no location. Saying so beats an empty field: the
    /// message alone is still most of the answer.
    #[test]
    fn a_panic_without_a_place_still_says_something() {
        let described = describe("something", None);

        assert!(described.contains("something"));
        assert!(described.len() > "something".len());
    }

    /// The hook must actually run. A panic in a spawned thread does not
    /// bring the process down, which is what makes this checkable at all.
    #[test]
    fn the_hook_runs_on_a_real_panic() {
        use std::sync::{Arc, Mutex};

        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);

        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let place = info.location().map(std::panic::Location::to_string);
            recorder
                .lock()
                .expect("the recorder")
                .push(describe(&payload(info), place.as_deref()));
        }));

        let joined = std::thread::spawn(|| panic!("deliberate, from a test")).join();

        std::panic::set_hook(previous);

        assert!(joined.is_err(), "the thread did panic");
        let seen = seen.lock().expect("the recorder");
        assert_eq!(seen.len(), 1, "the hook ran once");
        assert!(
            seen[0].contains("deliberate, from a test"),
            "the message reached it: {}",
            seen[0]
        );
        assert!(
            seen[0].contains("logging.rs"),
            "and so did the place: {}",
            seen[0]
        );
    }
}
