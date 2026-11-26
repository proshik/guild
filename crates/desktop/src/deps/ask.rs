//! Asking a build system what a project depends on — M8.14's task 2.
//!
//! Everything here may wait, and it is only ever called from the thread
//! `root::dependencies` starts: a Gradle that has to start its daemon and
//! download what it resolves takes minutes the first time, and the window
//! does not.
//!
//! **Commands run with the login shell's `PATH`**, as git does: a
//! `Guild.app` opened from Finder has no `/opt/homebrew/bin` otherwise,
//! and `cargo` and `go` would be *not found* there and nowhere else.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{Answer, Stack};

/// How long a build system is given. Gradle's first answer on a project
/// starts a daemon and may download everything it resolves; five minutes
/// is past that and short of a build that has hung.
pub const DEADLINE: Duration = Duration::from_secs(300);

/// Guild's init script, written where Gradle can be pointed at it.
const GRADLE_SCRIPT: &str = include_str!("../../assets/gradle/guild-deps.gradle");

/// Ask one stack of the project at `root`. `scratch` is a directory of
/// Guild's own where a script or an output file may be written.
pub fn ask(root: &Path, stack: Stack, scratch: &Path) -> Result<Answer, String> {
    match stack {
        Stack::Gradle => {
            let script = scratch.join("guild-deps.gradle");
            write_if_changed(&script, GRADLE_SCRIPT)?;
            let program = wrapper(root, "gradlew").unwrap_or_else(|| PathBuf::from("gradle"));
            let script = script.to_string_lossy().into_owned();
            let output = run(
                root,
                &program,
                &["-q", "--init-script", &script, "guildDependencies"],
                DEADLINE,
            )?;
            keep(scratch, stack, &output);
            Ok(super::gradle(&output))
        }
        Stack::Maven => {
            // `dependency:list` prints its list among Maven's own lines;
            // written to a file of ours it is the list alone. A reactor of
            // several modules appends to it, so it starts empty.
            let file = scratch.join("maven-dependencies.txt");
            let _ = std::fs::remove_file(&file);
            let program = wrapper(root, "mvnw").unwrap_or_else(|| PathBuf::from("mvn"));
            let output_file = format!("-DoutputFile={}", file.display());
            run(
                root,
                &program,
                &[
                    "-q",
                    // Batch mode: no colour and no progress. The reader
                    // takes colour out anyway — it was measured in the file.
                    "-B",
                    "dependency:list",
                    "-DoutputAbsoluteArtifactFilename=true",
                    "-DappendOutput=true",
                    &output_file,
                ],
                DEADLINE,
            )?;
            let text = std::fs::read_to_string(&file).map_err(|err| err.to_string())?;
            keep(scratch, stack, &text);
            Ok(super::maven(&text))
        }
        Stack::Cargo => {
            let output = run(
                root,
                Path::new("cargo"),
                &["metadata", "--format-version", "1"],
                DEADLINE,
            )?;
            keep(scratch, stack, &output);
            Ok(super::cargo(&output))
        }
        Stack::Go => super::go_vendor::discover(root),
        Stack::Npm => Ok(super::npm(root)),
        Stack::Bundler => Ok(super::bundler(root, &gem_roots())),
        Stack::Mix => Ok(super::mix(root)),
    }
}

/// What a stack answered last time, read back through the same reader —
/// M8.14's task 4, so a reopened project lists at once while the build
/// system is asked again. **The tool's own answer is kept, not a list of
/// ours**: the reader stays the one place that understands an answer, and
/// there is no second format to drift from it. The stacks read from
/// files are simply read again.
pub fn cached(root: &Path, stack: Stack, scratch: &Path) -> Option<Answer> {
    let kept = || std::fs::read_to_string(kept_at(scratch, stack)).ok();
    match stack {
        Stack::Gradle => kept().map(|text| super::gradle(&text)),
        Stack::Maven => kept().map(|text| super::maven(&text)),
        Stack::Cargo => kept().map(|text| super::cargo(&text)),
        // Go mode and workspace roots can change without touching this cache.
        Stack::Go => None,
        Stack::Npm => Some(super::npm(root)),
        Stack::Bundler => Some(super::bundler(root, &gem_roots())),
        Stack::Mix => Some(super::mix(root)),
    }
}

/// Where a stack's last answer is kept.
fn kept_at(scratch: &Path, stack: Stack) -> PathBuf {
    scratch.join(format!("{stack:?}.answer").to_lowercase())
}

/// Keep an answer for next time. A failure to write is a slower next
/// start, not an error anybody needs to see.
fn keep(scratch: &Path, stack: Stack, answer: &str) {
    if let Err(err) = write_if_changed(&kept_at(scratch, stack), answer) {
        tracing::debug!(?stack, %err, "an answer was not kept");
    }
}

/// Run a program in `root` and answer what it printed, or why not.
///
/// **Waited for with a deadline, and killed at it** — the person chose a
/// `try_wait` loop of our own over a crate, as the terminal and the
/// language servers already do. Both streams are read on threads of their
/// own: a program whose pipe fills while nobody reads it waits for ever,
/// and would reach the deadline for no reason of its own.
pub fn run(
    root: &Path,
    program: &Path,
    args: &[&str],
    deadline: Duration,
) -> Result<String, String> {
    run_with(root, program, args, &[], deadline)
}

/// [`run`], with variables set for the program — `DEVELOPER_DIR` for
/// `xcode-build-server`, 2026-09-28.
///
/// A program that is not on the login shell's `PATH` is said as *not
/// found*, the words the status bar uses for a missing server, rather
/// than as the operating system's error number.
pub fn run_with(
    root: &Path,
    program: &Path,
    args: &[&str],
    env: &[(&str, &std::ffi::OsStr)],
    deadline: Duration,
) -> Result<String, String> {
    let name = program.display().to_string();
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = guild_session::login_path::path() {
        command.env("PATH", path);
    }
    command.envs(env.iter().copied());
    crate::network::apply(&mut command, guild_session::network::Scope::Tools)
        .map_err(str::to_owned)?;
    let mut child = guild_session::process::ProcessGroup::spawn(&mut command).map_err(|err| {
        match err.kind() {
            std::io::ErrorKind::NotFound => format!("{name} not found"),
            _ => format!("{name}: {err}"),
        }
    })?;

    let mut stdout = child.take_stdout().ok_or("no output")?;
    let mut stderr = child.take_stderr().ok_or("no errors")?;
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });

    let until = Instant::now() + deadline;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= until => {
                child.stop().map_err(|error| format!("{name}: {error}"))?;
                return Err(format!(
                    "{name} did not answer in {} s",
                    deadline.as_secs_f32()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(format!("{name}: {error}")),
        }
    };
    let out = out
        .join()
        .map_err(|_| format!("{name}: output reader stopped"))?
        .map_err(|error| format!("{name}: could not read output: {error}"))?;
    let err = err
        .join()
        .map_err(|_| format!("{name}: diagnostic reader stopped"))?
        .map_err(|error| format!("{name}: could not read diagnostics: {error}"))?;
    if status.success() {
        // stdout is machine-readable dependency data: silently replacing bytes
        // could corrupt an identifier or path. Diagnostics have no such contract.
        String::from_utf8(out).map_err(|error| format!("{name}: output is not UTF-8: {error}"))
    } else {
        // Preserve the process outcome and readable diagnostic tail even when
        // a tool uses a different locale or emits non-text bytes on either pipe.
        let diagnostics = String::from_utf8_lossy(&err);
        let why: Vec<&str> = diagnostics
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        let tail = why[why.len().saturating_sub(3)..].join(" ");
        Err(format!("{name} exited with {status}: {tail}"))
    }
}

/// The project's own wrapper — `./gradlew`, `./mvnw` — which pins the
/// version the project is built with, where there is one.
fn wrapper(root: &Path, name: &str) -> Option<PathBuf> {
    let path = root.join(name);
    path.is_file().then_some(path)
}

/// Write a file only when it says something else, so that Gradle's own
/// caching of an init script is not defeated by a new modification time.
fn write_if_changed(path: &Path, text: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    std::fs::write(path, text).map_err(|err| err.to_string())
}

/// Where Rubies keep their gems, each a directory with `gems/` in it.
///
/// **Looked for rather than asked**: the `ruby` on a login `PATH` is
/// often macOS's own 2.6 while the project's gems were installed by
/// Homebrew's Ruby or a version manager's — measured in task 0.
pub fn gem_roots() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let patterns: [PathBuf; 8] = [
        PathBuf::from("/opt/homebrew/lib/ruby/gems"),
        PathBuf::from("/usr/local/lib/ruby/gems"),
        home.join(".gem/ruby"),
        home.join(".local/share/gem/ruby"),
        home.join(".rbenv/versions"),
        home.join(".asdf/installs/ruby"),
        home.join(".rubies"),
        PathBuf::from("/Library/Ruby/Gems"),
    ];
    let mut roots = Vec::new();
    for base in patterns {
        for entry in super::children(&base) {
            // A version manager keeps a whole Ruby per version; its gems
            // are further in.
            let nested = entry.join("lib/ruby/gems");
            if nested.is_dir() {
                roots.extend(super::children(&nested));
            } else if entry.join("gems").is_dir() {
                roots.push(entry);
            }
        }
    }
    roots
}
