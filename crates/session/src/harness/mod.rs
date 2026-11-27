//! Which agent CLI a session tab runs, and what it can do — M14.
//!
//! The harnesses are the agent CLIs Guild runs in a session tab: Claude
//! Code, OpenCode and Codex. They share a common control model — `Status`,
//! `Signal`, `SessionMode`, continuing a session — and differ only in
//! *how* a raw signal is translated into a `Signal` and *how* the CLI is
//! launched or resumed. That is what this module is: the interface, and a
//! concrete implementation per harness, beside it.
//!
//! Where a harness does not have a capability, the interface answers a
//! default or `None`. Today only the session core is wired: launch, resume,
//! approval, status and detection. The MCP/IDE bridge is shaped as `ide()`
//! and answers `None` for all three; cost and transcripts answer `None` for
//! the two new harnesses until that work lands.
//!
//! **No GPUI.** The crate is a vocabulary that must be checkable without a
//! window, and this module is pure: it builds argument lists and maps
//! envelope JSON into signals, nothing more. The desktop owns the pty and
//! the hook registration and calls this.

mod claude;
mod codex;
mod opencode;
mod selection;
pub use selection::LaunchSelection;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::PermissionMode;

pub use claude::ClaudeHarness;
pub use codex::CodexHarness;
pub use opencode::OpenCodeHarness;

/// Whether an agent is **known** on this machine: its own directory —
/// [`CodexHarness::home`], [`OpenCodeHarness::config_dir`] for its plugin,
/// [`OpenCodeHarness::data_dir`] for its sessions — exists. GLD-078, the
/// person's decision "only the agents already known": the window writes into
/// an agent's directory, and runs the agent in the background, only for an
/// agent known here, or one the person starts from Guild. A program on the
/// `PATH` that was never run is not known — running it, or writing its
/// config, is what makes its directory, and the first run is the person's.
///
/// The one rule, so the start's writes, the limits card and the start
/// screen's history cannot part on what "known" means.
pub fn known(dir: &Path) -> bool {
    !dir.as_os_str().is_empty() && dir.is_dir()
}

/// Each agent's own directory, where Guild writes its status door — Codex's
/// home for the hook block, opencode's config directory for the plugin. The
/// real ones from [`AgentDirs::here`], a test's own otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDirs {
    /// [`CodexHarness::home`]: `$CODEX_HOME`, else `~/.codex`.
    pub codex: PathBuf,
    /// [`OpenCodeHarness::config_dir`]: `$XDG_CONFIG_HOME/opencode`, else
    /// `~/.config/opencode`.
    pub opencode: PathBuf,
}

impl AgentDirs {
    /// This machine's, by the environment.
    pub fn here() -> Self {
        Self {
            codex: CodexHarness::home(),
            opencode: OpenCodeHarness::config_dir(),
        }
    }

    /// The directory `harness`'s door is written into — `None` for Claude,
    /// whose hooks are handed on its command line and written into nothing
    /// of its own.
    pub fn of(&self, harness: HarnessKind) -> Option<&Path> {
        match harness {
            HarnessKind::Claude => None,
            HarnessKind::Codex => Some(&self.codex),
            HarnessKind::OpenCode => Some(&self.opencode),
        }
    }

    /// Whether `harness` is [`known`] here; Claude, with nothing written of
    /// its own, never is.
    pub fn known(&self, harness: HarnessKind) -> bool {
        self.of(harness).is_some_and(known)
    }
}

/// Which agent CLI a session tab runs.
///
/// Persisted through [`crate::Runs`], whose variants are the wire format:
/// a name here is data in every project's `state.json`, so versions are
/// added, never renamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessKind {
    /// Claude Code.
    Claude,
    /// OpenCode.
    OpenCode,
    /// Codex.
    Codex,
}

impl HarnessKind {
    /// Every harness, in a stable order — what the UI's choice menu lists.
    pub const ALL: [HarnessKind; 3] = [Self::Claude, Self::OpenCode, Self::Codex];

    /// The concrete implementation of this harness.
    pub fn impl_(self) -> &'static dyn Harness {
        match self {
            Self::Claude => &ClaudeHarness,
            Self::OpenCode => &OpenCodeHarness,
            Self::Codex => &CodexHarness,
        }
    }

    /// The program on the `PATH`.
    pub fn program(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::OpenCode => "opencode",
            Self::Codex => "codex",
        }
    }

    /// The agent's own name, as a person knows it — what the command
    /// line's refusals say (`command::not_installed`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::OpenCode => "OpenCode",
            Self::Codex => "Codex",
        }
    }

    /// Where a person gets this agent: its own install page — GLD-081.
    /// What the notice for a missing agent and README's *Requirements*
    /// name; each was checked to resolve on 2026-11-27.
    pub fn install_page(self) -> &'static str {
        match self {
            Self::Claude => "https://code.claude.com/docs/en/setup",
            Self::OpenCode => "https://opencode.ai/docs",
            Self::Codex => "https://github.com/openai/codex",
        }
    }

    /// Which of `ALL` are on the given `PATH` — what the harness-choice
    /// menu offers as enabled rows. Pure, so the menu's state is testable
    /// without a window.
    pub fn available_on(path: &str) -> Vec<HarnessKind> {
        Self::ALL
            .iter()
            .copied()
            .filter(|&kind| kind.is_on(path))
            .collect()
    }

    /// Whether this harness's program is on the given `PATH` — one of
    /// [`Self::available_on`]'s answers, asked alone.
    pub fn is_on(self, path: &str) -> bool {
        on_path(self.program(), path)
    }

    /// The `(major, minor)` version its CLI reports, by running
    /// `<program> --version` on the given `PATH`. `None` when it cannot be
    /// told.
    ///
    /// Waited for with a deadline and killed at it — the deps asker's rule,
    /// the person's choice of a `try_wait` loop of our own. An `opencode`
    /// that never answers would otherwise hold this function, and the
    /// window that called it, for ever.
    pub fn probe_version(&self, path: &str) -> Option<(u32, u32)> {
        use std::io::Read;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let mut command = Command::new(self.program());
        command
            .arg("--version")
            .env("PATH", path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = crate::process::ProcessGroup::spawn(&mut command).ok()?;
        let out = {
            let mut pipe = child.take_stdout()?;
            std::thread::spawn(move || {
                let mut out = Vec::new();
                pipe.read_to_end(&mut out).ok().map(|_| out)
            })
        };
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let out = out.join().ok()??;
                    return status.success().then(|| parse_version(&out)).flatten();
                }
                // Not asked to answer in time: no harness is worth a frozen
                // window. Kill, and treat the version as unknown.
                Ok(None) if Instant::now() >= until => {
                    let _ = child.stop();
                    return None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => return None,
            }
        }
    }

    /// The required version a harness has a floor for — `None` for a
    /// harness with no minimum. What Guild relies on (a status plugin, a
    /// flag's shape) can live above a floor that this build was measured
    /// against, and below it the harness must not be offered as if it were.
    pub fn floor_miss(&self, path: &str) -> Option<(u32, u32)> {
        let floor = self.impl_().version_floor()?;
        let installed = self.probe_version(path).unwrap_or((0, 0));
        (installed < floor).then_some(floor)
    }
}

/// The first `major.minor` version number anywhere in `bytes`, or `None`.
///
/// The CLIs print their version in their own prose — `opencode v2.0.10`,
/// `codex-cli 0.155.1`, `2.1.278 (Claude Code)` — and the one thing they
/// share is a number. Pure, so it is testable without a process.
pub fn parse_version(bytes: &[u8]) -> Option<(u32, u32)> {
    let chars: Vec<char> = String::from_utf8_lossy(bytes).chars().collect();
    let read_number = |at: &mut usize, chars: &[char]| -> Option<u32> {
        let mut n = 0u32;
        while *at < chars.len() && chars[*at].is_ascii_digit() {
            n = n.checked_mul(10)?.checked_add(chars[*at].to_digit(10)?)?;
            *at += 1;
        }
        Some(n)
    };
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut at = i;
            let major = read_number(&mut at, &chars)?;
            if at + 1 < chars.len() && chars[at] == '.' && chars[at + 1].is_ascii_digit() {
                at += 1;
                let minor = read_number(&mut at, &chars)?;
                return Some((major, minor));
            }
            return (major > 0).then_some((major, 0));
        }
        i += 1;
    }
    None
}

/// Whether `program` is an executable on the `:`-separated `PATH`.
fn on_path(program: &str, path: &str) -> bool {
    path.split(':').any(|dir| {
        let candidate = std::path::Path::new(dir).join(program);
        std::fs::metadata(candidate)
            .map(|meta| meta.is_file() && std::os::unix::fs::MetadataExt::mode(&meta) & 0o111 != 0)
            .unwrap_or(false)
    })
}

/// What a harness can do.
///
/// A session tab asks these questions and nothing else; the answers differ
/// per harness, and a new harness is a new implementation of this trait.
pub trait Harness {
    /// Which harness this is.
    fn kind(&self) -> HarnessKind;

    /// Whether `args` is this harness running **in front of** a pty — the
    /// command line of the foreground process group, as a shell typed it.
    ///
    /// The `claude` on the `PATH` can be somebody else's (a superconductor
    /// wrapper), so the wrappers are recognised too, the way
    /// `is_claude_command` did before M14.
    fn detect_in_front(&self, args: &str) -> bool;

    /// The conversation `args` resumes by id, when this harness is in
    /// front and its command line names one — M13.3. What a terminal tab
    /// is recognised by when a person types a resume into it; a resume
    /// that names no id — a picker, `--continue`, `--last` — is `None`,
    /// because the command line does not say which.
    fn resumed_in(&self, args: &str) -> Option<String>;

    /// Whether a process with this command line is writing `conversation` —
    /// what `guild session resume` refuses on, M13.3.
    fn holds(&self, args: &str, conversation: &str) -> bool {
        self.resumed_in(args).as_deref() == Some(conversation)
    }

    /// The arguments that continue an unfinished session by id.
    fn resume_args(&self, id: &str) -> Vec<OsString>;

    /// The arguments that express a permission mode, if the harness has
    /// one. Guild’s PermissionMode is Claude vocabulary; Codex-specific
    /// settings are composed by the desktop launcher, and OpenCode retains
    /// its native permission choices.
    fn approval_args(&self, permission: PermissionMode) -> Vec<OsString>;

    /// The arguments a fresh session is given that are neither resume nor
    /// approval. Most harnesses need none.
    fn base_args(&self) -> Vec<OsString> {
        Vec::new()
    }

    /// The minimum `(major, minor)` CLI version Guild was measured against
    /// and relies on — a status plugin's shape, a flag's name. `None` for a
    /// harness with no floor. OpenCode's is `2`, because its V2 plugin and
    /// CLI are what the status work is built on.
    fn version_floor(&self) -> Option<(u32, u32)> {
        None
    }

    /// The MCP/IDE bridge this harness could speak. `None` for all three
    /// this milestone — the interface is shaped so the work can land later
    /// without touching its consumers.
    fn ide(&self) -> Option<()> {
        None
    }
}

/// Whether `word` names this harness, or a wrapper of it, by its file name.
///
/// A path's last component is the name (`/share/claude/versions/x` ends in
/// `claude`), and a wrapper script's own path ends in the harness's name.
pub(crate) fn word_names_program(program: &str, word: &str) -> bool {
    let name = word.rsplit('/').next().unwrap_or(word);
    name == program
        || word.contains(&format!("/super.engineering/bin/{program}"))
        || word.contains(&format!("/superconductor/bin/{program}"))
}

#[cfg(test)]
mod tests;

/// The value of the first of `flags` in `args`, written `FLAG VALUE` or
/// `FLAG=VALUE`; a next word that is itself a flag is no value — M13.3.
pub(crate) fn flag_value(args: &str, flags: &[&str]) -> Option<String> {
    let mut words = args.split_whitespace();
    while let Some(word) = words.next() {
        for flag in flags {
            if word == *flag {
                return words
                    .next()
                    .filter(|value| !value.starts_with('-'))
                    .map(str::to_owned);
            }
            if let Some(value) = word
                .strip_prefix(flag)
                .and_then(|rest| rest.strip_prefix('='))
            {
                return (!value.is_empty()).then(|| value.to_owned());
            }
        }
    }
    None
}

/// Every process in `ps -axww -o pid=,ppid=,args=`'s output that holds
/// `conversation`, as `(pid, parent, command line)` — M13.3. A line that is
/// not two numbers and a command is skipped. Parent is snapshot data only:
/// a harness can be reparented to PID 1 when its wrapper shell exits. A caller
/// granting closing-holder grace must use its retained cleanup witness (SID
/// and captured process groups), not PPID, to establish ownership.
///
/// **A command line is split on whitespace**, and Guild's own tabs carry
/// their appended system prompt on it whole — measured in M13.3's task 0;
/// none of its words is a resume flag.
pub fn holders_with_parents(ps: &str, conversation: &str) -> Vec<(u32, u32, String)> {
    ps.lines()
        .filter_map(|line| {
            let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
            let (parent, args) = rest.trim_start().split_once(char::is_whitespace)?;
            Some((
                pid.parse().ok()?,
                parent.parse().ok()?,
                args.trim().to_owned(),
            ))
        })
        .filter(|(_, _, args)| {
            HarnessKind::ALL
                .iter()
                .any(|kind| kind.impl_().holds(args, conversation))
        })
        .collect()
}
