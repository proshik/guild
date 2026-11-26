//! The selected launcher observer has a real process owner, separate from its job.
use super::*;
use crate::{SessionId, process::ProcessGroup};
use std::{
    ffi::OsString,
    os::unix::{
        fs::{FileTypeExt, OpenOptionsExt, PermissionsExt},
        process::ExitStatusExt,
    },
    process::{Command, Stdio},
};

/// Actual native owning-window context. The Root adapter supplies this from its
/// live origin/socket; a model payload cannot create or replace these bindings.
#[derive(Clone)]
pub struct RevustHost {
    /// Original initiating session, never an assigned builder identity.
    pub session: SessionId,
    /// Actual owning window's private control socket.
    pub socket: PathBuf,
    /// Qualified public Guild CLI; its directory alone is prepended to child PATH.
    pub cli: PathBuf,
}
/// Durable observer facts. Its exit never implies native job cancellation or a
/// clean review; the coordinator separately reconciles job/archive/actual calls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevustLauncherExit {
    /// Observed owned launcher process identity, not authority to signal a PID.
    pub pid: u32,
    /// Actual normal exit code, absent for signals.
    pub exit_code: Option<i32>,
    /// Actual terminating signal, if observed.
    pub signal: Option<i32>,
    /// Retained report stream, kept separate from diagnostics.
    pub stdout: Artifact,
    /// Retained diagnostic stream; never relayed as a provider environment.
    pub stderr: Artifact,
}
/// Live nonpersistable process ownership. No recovery path recreates this owner
/// from a saved PID. Dropping it stops/reaps its observer group only; a Guild
/// terminal job remains under the native terminal registry's independent owner.
pub struct RevustLauncher {
    group: ProcessGroup,
    stdout: PathBuf,
    stderr: PathBuf,
    observed: Option<RevustLauncherExit>,
}
impl RevustLauncher {
    /// Start only after the coordinator durably reserves its enforceable review
    /// allowance and freshly qualifies source/parent authority. This process
    /// wrapper does not itself grant execution authority or own the native job.
    pub fn spawn(round: &RevustRound, host: &RevustHost) -> Result<Self, String> {
        round.verify()?;
        if let Some(environment) = &round.environment {
            environment.verify()?;
        }

        if !host.socket.is_absolute()
            || !std::fs::metadata(&host.socket)
                .map_err(|e| format!("revust.host socket: {e}"))?
                .file_type()
                .is_socket()
        {
            return Err("revust.host: actual owning socket is unavailable".into());
        }
        if !host.cli.is_absolute()
            || host.cli.file_name() != Some(std::ffi::OsStr::new("guild"))
            || !std::fs::metadata(&host.cli)
                .map_err(|e| format!("revust.host CLI: {e}"))?
                .is_file()
            || std::fs::metadata(&host.cli)
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o111
                == 0
        {
            return Err("revust.host: qualified executable Guild CLI required".into());
        }
        let directory = round.archive.join("workflow-observer");
        std::fs::create_dir(&directory).map_err(|e|format!("revust.launch: fresh observer artifacts required; reconcile before another start: {e}"))?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let stdout = directory.join("stdout");
        let stderr = directory.join("stderr");
        let out = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&stdout)
            .map_err(|e| e.to_string())?;
        let err = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&stderr)
            .map_err(|e| e.to_string())?;
        let inherited = crate::login_path::path()
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_else(|| OsString::from("/usr/bin:/bin"));
        let path = std::env::join_paths(
            std::iter::once(
                host.cli
                    .parent()
                    .ok_or("revust.host: CLI parent missing")?
                    .to_path_buf(),
            )
            .chain(std::env::split_paths(&inherited)),
        )
        .map_err(|e| e.to_string())?;
        let mut command = Command::new(round.package.launcher());
        command
            .args(&round.args)
            .current_dir(&round.configuration.directory)
            .env("REVUST_BIN", round.package.program())
            .env("REVUST_HOST", "guild")
            .env("GUILD_SESSION", host.session.to_string())
            .env("GUILD_SOCKET", &host.socket)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PATH", path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err));
        let group=ProcessGroup::spawn(&mut command).map_err(|e|format!("revust.launch: selected observer failed to spawn; no repeat without reconciliation: {e}"))?;
        Ok(Self {
            group,
            stdout,
            stderr,
            observed: None,
        })
    }
    /// Actual process observation only; never a terminal-job identity.
    pub fn id(&self) -> u32 {
        self.group.id()
    }
    /// Nonblocking owned-process polling. File inspection belongs off the UI
    /// thread; oversized streams stay retained and cannot establish success.
    pub fn try_wait(&mut self) -> Result<Option<RevustLauncherExit>, String> {
        if let Some(observed) = &self.observed {
            return Ok(Some(observed.clone()));
        }
        let Some(status) = self
            .group
            .try_wait()
            .map_err(|e| format!("revust.launch: owned observer state unknown: {e}"))?
        else {
            return Ok(None);
        };
        let artifact = |path: &Path, max: u64| -> Result<Artifact, String> {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let metadata = file.metadata().map_err(|e| e.to_string())?;
            if !metadata.is_file() || metadata.len() > max {
                return Err("revust.launch: retained observer stream exceeds bounds; inspect actual job/archive, no rerun".into());
            }
            file.sync_all().map_err(|e| e.to_string())?;
            Ok(Artifact {
                path: path.to_path_buf(),
                sha256: file_digest(path)?,
            })
        };
        let observed = RevustLauncherExit {
            pid: self.group.id(),
            exit_code: status.code(),
            signal: status.signal(),
            stdout: artifact(&self.stdout, 64 * 1024 * 1024)?,
            stderr: artifact(&self.stderr, 16 * 1024 * 1024)?,
        };
        self.observed = Some(observed.clone());
        Ok(Some(observed))
    }
}
