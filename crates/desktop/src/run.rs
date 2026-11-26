//! A command a person runs, and what became of it.
//!
//! The model of M8.4's Output panel, and pure: a line of text becomes an
//! argv, a run becomes one of four states, a duration becomes something a
//! person reads off a clock. What runs the argv is
//! [`crate::terminal::view::open_command`], the same door a terminal tab
//! goes through — decision 1 of the plan.
//!
//! **A command is a line, not a program and its arguments.** It is handed
//! to the person's login shell, which is where their `PATH`, their
//! aliases and their functions are. Not for convenience: `just` lives in
//! `/opt/homebrew/bin`, and a bundled `Guild.app` inherits a `PATH` from
//! `launchd` that does not have it — run directly, the button this
//! milestone is named after would be *not found*, and only in the bundle.

pub mod discover;
pub mod source;
pub mod tabs;

use std::time::Duration;

use portable_pty::CommandBuilder;

/// Whether there is anything to run.
///
/// A blank line handed to a shell is a process that starts, exits at once
/// and reports success — a run that says everything is fine about nothing
/// at all.
pub fn runnable(line: &str) -> bool {
    !line.trim().is_empty()
}

/// The argv a line becomes: the shell, a login flag, `-c`, and the line
/// itself, whole.
///
/// The shell is chosen by the rule a terminal tab uses —
/// [`crate::terminal::shell::program`] — so that the tab and the panel
/// cannot end up in different shells.
pub fn argv(line: &str, shell: Option<&str>) -> Vec<String> {
    vec![
        crate::terminal::shell::program(shell).to_owned(),
        "-l".to_owned(),
        "-c".to_owned(),
        line.to_owned(),
    ]
}

/// The command to spawn, or nothing when there is nothing to run.
///
/// The shell comes from `SHELL`, which is where the system records the
/// person's choice at login; [`argv`] is where the rule lives, so that it
/// can be checked without touching the process environment.
pub fn command(line: &str) -> Option<CommandBuilder> {
    if !runnable(line) {
        return None;
    }
    let shell = std::env::var("SHELL").ok();
    let argv = argv(line, shell.as_deref());
    let mut command = CommandBuilder::new(&argv[0]);
    for argument in &argv[1..] {
        command.arg(argument);
    }
    Some(command)
}

/// What has become of a run.
///
/// Four states rather than a code and a flag: *stopped* is a fact the
/// window holds because the window is what stopped it. Task 0 of M8.4
/// measured the alternative — a killed process reports exit code 1, which
/// is what a command that genuinely failed with 1 reports — so a stop
/// read back out of the status would be a guess dressed as a fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// The process is alive.
    Running,
    /// It ended by itself, with this code.
    Finished {
        /// What it exited with. Zero is the only good one.
        code: u32,
    },
    /// We stopped it.
    Stopped,
    /// It never started: the shell is missing, the pty could not be
    /// opened. The reason is the error's own words.
    Failed {
        /// What went wrong, as the failure said it.
        reason: String,
    },
}

impl State {
    /// Whether there is nothing more to wait for.
    pub fn is_over(&self) -> bool {
        !matches!(self, State::Running)
    }

    /// Whether it is the one outcome that needs no reading.
    pub fn went_well(&self) -> bool {
        matches!(self, State::Finished { code: 0 })
    }
}

/// How long a run took, the way a person reads a clock.
///
/// Seconds alone under a minute; then minutes with the seconds padded, so
/// the number keeps its width instead of jumping about while it counts.
pub fn elapsed(taken: Duration) -> String {
    let seconds = taken.as_secs();
    match seconds {
        ..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60),
    }
}

/// Where a row of the list came from.
///
/// The three are drawn differently and mean different things: what the
/// person is typing, what they have run here before, and what the project
/// itself declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// What was typed, which is not in the list yet.
    Typed,
    /// Run here lately.
    Recent,
    /// Declared by a `justfile` or a `package.json`.
    Declared,
}

/// One row of the list under the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The line it would run.
    pub line: String,
    /// Where it came from.
    pub source: Source,
}

/// The list under the field: what was typed and is not there yet, then
/// what this project has run lately, then what it declares.
///
/// **A line the project declares and the person has run is one row**, and
/// it is the recent one — the list is ordered by what a person is likely
/// to reach for, and having run it is the stronger evidence.
///
/// The offer to run what was typed is the branch popup's create row in
/// another hat, and it appears whenever the query is not already a row —
/// not merely when nothing matched: typing `just c` in a project with
/// `just check` matches, and the offer to run `just c` is still the right
/// one to show, because a command is a line and not a name.
///
/// **Found by scattered letters, best first within each source** since
/// M8.9 — `jck` is `just check`. The sources keep their order: what was
/// typed, then what was run, then what is declared, because the order is
/// what a person is likely to reach for and a better match further down
/// does not change that.
pub fn rows(query: &str, recent: &[String], declared: &[String]) -> Vec<Row> {
    let typed = query.trim();
    let known = |line: &str| line == typed;

    let mut rows = Vec::new();
    if !typed.is_empty()
        && !recent.iter().any(|line| known(line))
        && !declared.iter().any(|line| known(line))
    {
        rows.push(Row {
            line: typed.to_owned(),
            source: Source::Typed,
        });
    }
    let found = |lines: &[String]| {
        crate::fuzzy::rank(typed, lines, String::as_str, crate::fuzzy::Shape::Name)
            .into_iter()
            .map(|(index, _)| index)
    };
    rows.extend(found(recent).map(|index| Row {
        line: recent[index].clone(),
        source: Source::Recent,
    }));
    rows.extend(
        found(declared)
            .filter(|index| !recent.contains(&declared[*index]))
            .map(|index| Row {
                line: declared[index].clone(),
                source: Source::Declared,
            }),
    );
    rows
}
