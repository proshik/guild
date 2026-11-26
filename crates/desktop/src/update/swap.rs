//! The click's half that touches the disk and starts a process — M5.15.
//!
//! [`prepare`] makes the staged bundle the window's own: a copy in a
//! folder of its own, checked and read back, with the helper beside it.
//! [`start_helper`] starts that helper in a session of its own, told this
//! window's pid and the folder to remove when it is done, and the window
//! then quits. Callable from a test with
//! fixture folders; the window calls it from `root::update`, the copy on
//! a thread.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{Build, offered};

/// The helper's name, in a bundle's `Contents/Resources` and in the
/// folder it runs from.
pub const HELPER: &str = "swap-guild.sh";

/// The helper a bundle carries — written there by `just bundle`.
pub fn helper_of(bundle: &Path) -> PathBuf {
    bundle.join("Contents").join("Resources").join(HELPER)
}

/// Where the click copies the staged bundle: under the system's
/// temporary folder, named by the window's pid.
pub fn folder(pid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("guild-update-{pid}"))
}

/// What the helper writes, for whoever wants to know how a swap went.
pub fn log() -> PathBuf {
    std::env::temp_dir().join("guild-update.log")
}

/// What the click has made ready: a copy of the staged bundle and the
/// helper beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The folder the click made — the helper removes it once the swap
    /// is over, the helper's own file included.
    pub folder: PathBuf,
    /// `<folder>/Guild.app`, the copy.
    pub bundle: PathBuf,
    /// `<folder>/swap-guild.sh`, the running bundle's own.
    pub helper: PathBuf,
    /// What the copy says it is.
    pub build: Build,
}

/// Why an update was not made ready. Words for a notice are the
/// window's; the tools' own are carried as they said them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The running bundle carries no helper to swap with.
    NoHelper,
    /// The copy failed — `ditto`'s words, or the file system's.
    Copy(String),
    /// The copy's seal does not hold — `codesign`'s words. Two builds
    /// mixed by a copy that `just install` renamed a bundle under, or a
    /// bundle whose signing failed.
    Seal(String),
    /// What was copied is not a bundle at all.
    NotABuild,
    /// What was copied is not newer than what runs.
    NotNewer,
}

/// Step 4 of decision 7 of M5.15's spec: `ditto` the staged bundle into
/// `dir`, verify its seal, read it back — it must still be offered over
/// `running` — and put the running bundle's helper beside it. **A copy is
/// judged, not the staged folder**: `just install` may replace that while
/// this runs, and a copy that caught a newer build is still an update.
/// On any failure `dir` is removed: a no leaves nothing behind.
pub fn prepare(
    staged: &Path,
    running_bundle: &Path,
    running: Option<&Build>,
    dir: &Path,
) -> Result<Prepared, Failure> {
    let helper = helper_of(running_bundle);
    if !helper.is_file() {
        return Err(Failure::NoHelper);
    }
    let _ = std::fs::remove_dir_all(dir);
    let made = copy(staged, &helper, running, dir);
    if made.is_err() {
        let _ = std::fs::remove_dir_all(dir);
    }
    made
}

fn copy(
    staged: &Path,
    helper: &Path,
    running: Option<&Build>,
    dir: &Path,
) -> Result<Prepared, Failure> {
    std::fs::create_dir_all(dir).map_err(|err| Failure::Copy(err.to_string()))?;
    let bundle = dir.join("Guild.app");
    tool(Command::new("ditto").arg(staged).arg(&bundle)).map_err(Failure::Copy)?;
    tool(
        Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&bundle),
    )
    .map_err(Failure::Seal)?;
    let build = Build::of_bundle(&bundle).ok_or(Failure::NotABuild)?;
    if !offered(running, &build) {
        return Err(Failure::NotNewer);
    }
    let beside = dir.join(HELPER);
    std::fs::copy(helper, &beside).map_err(|err| Failure::Copy(err.to_string()))?;
    Ok(Prepared {
        folder: dir.to_path_buf(),
        bundle,
        helper: beside,
        build,
    })
}

/// Run a tool; its own words when it fails.
fn tool(command: &mut Command) -> Result<(), String> {
    let out = command
        .stdin(Stdio::null())
        .output()
        .map_err(|err| err.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    Err(if said.is_empty() {
        out.status.to_string()
    } else {
        said
    })
}

/// Step 5's command line: `sh <helper> <copy> <running bundle> <pid>
/// <folder>`. The folder is named rather than left for the script to work
/// out from `$0`: the same script runs from `scripts/` for `just install`,
/// and there the folder it lives in is a checkout's.
pub fn helper_command(prepared: &Prepared, running_bundle: &Path, pid: u32) -> Command {
    let mut command = Command::new("sh");
    command
        .arg(&prepared.helper)
        .arg(&prepared.bundle)
        .arg(running_bundle)
        .arg(pid.to_string())
        .arg(&prepared.folder);
    command
}

/// Step 5: [`helper_command`], detached. It waits for `pid` to exit,
/// swaps, opens the new Guild and removes the folder — so it must outlive
/// the window: **a session of its own**, so neither the window's
/// exit nor the terminal a window was started from takes it down; stdin
/// from nowhere; what it says to [`log`].
pub fn start_helper(prepared: &Prepared, running_bundle: &Path, pid: u32) -> std::io::Result<()> {
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log())?;
    let mut command = helper_command(prepared, running_bundle, pid);
    command
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // SAFETY: `setsid` is async-signal-safe, which is the whole of what
    // `pre_exec` asks for; it runs in the forked child before exec and
    // makes it the leader of a session with no terminal.
    unsafe {
        use std::os::unix::process::CommandExt as _;
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    // Not waited for: it is meant to outlive us. The zombie it would
    // leave if it ended first is reaped by nobody for the second or so
    // the window has left.
    command.spawn().map(drop)
}
