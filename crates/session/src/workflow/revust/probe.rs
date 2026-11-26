//! Fixed preparation operations only. No entry point executes a review/provider.
use super::*;
use crate::process::ProcessGroup;
use std::{
    ffi::OsString,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn drain(mut pipe: impl Read) -> Result<(Vec<u8>, bool), String> {
    let mut kept = Vec::new();
    let mut block = [0u8; 16384];
    let mut truncated = false;
    loop {
        let n = pipe.read(&mut block).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let available = (4 * 1024 * 1024usize).saturating_sub(kept.len());
        let take = available.min(n);
        kept.extend_from_slice(&block[..take]);
        truncated |= take < n;
    }
    Ok((kept, truncated))
}
impl RevustPackage {
    pub(super) fn preparation(&self, caller: &Path, args: &[OsString]) -> Result<Vec<u8>, String> {
        self.verify()?;
        if !caller.is_absolute() || !caller.is_dir() {
            return Err("revust.prepare: qualified existing caller cwd required".into());
        }
        let mut command = Command::new(self.program());
        command
            .args(args)
            .current_dir(caller)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut group =
            ProcessGroup::spawn(&mut command).map_err(|e| format!("revust.prepare: {e}"))?;
        let stdout = group
            .take_stdout()
            .ok_or("revust.prepare: stdout unavailable")?;
        let stderr = group
            .take_stderr()
            .ok_or("revust.prepare: stderr unavailable")?;
        let out = std::thread::spawn(move || drain(stdout));
        let err = std::thread::spawn(move || drain(stderr));
        let until = Instant::now() + Duration::from_secs(15);
        let mut interrupted = None;
        let status = loop {
            match group.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(5)),
                Ok(None) => {
                    interrupted =
                        Some("revust.prepare: deadline; owned process group stopped".to_owned());
                    let _ = group.stop();
                    break None;
                }
                Err(error) => {
                    interrupted =
                        Some(format!("revust.prepare: child observation failed: {error}"));
                    let _ = group.stop();
                    break None;
                }
            }
        };
        // Stop/reap precedes joining readers, so descendants cannot retain pipes.
        let output = out
            .join()
            .map_err(|_| "revust.prepare: output reader panicked")??;
        let diagnostics = err
            .join()
            .map_err(|_| "revust.prepare: diagnostic reader panicked")??;
        if let Some(error) = interrupted {
            return Err(error);
        }
        if output.1 || diagnostics.1 {
            return Err(
                "revust.prepare: bounded output capture exceeded; no dependent execution admitted"
                    .into(),
            );
        }
        let status = status.ok_or("revust.prepare: no actual exit status")?;
        if !status.success() {
            // Inspection may reject private configuration. Retain its digest,
            // never echo a provider environment value or authored source line.
            return Err(format!(
                "revust.prepare: selected installed command failed (exit {:?}, diagnostic sha256={})",
                status.code(),
                digest(&diagnostics.0)
            ));
        }
        self.verify()?;
        Ok(output.0)
    }
    /// Export only embedded defaults into a fresh explicitly owned directory.
    /// This is preparation, never installation or profile/provider execution.
    pub fn export_defaults(&self, caller: &Path, destination: &Path) -> Result<(), String> {
        if !destination.is_absolute() || destination.try_exists().map_err(|e| e.to_string())? {
            return Err("revust.prepare: fresh absolute owned defaults directory required".into());
        }
        self.preparation(
            caller,
            &["--dump-defaults".into(), destination.as_os_str().into()],
        )?;
        Ok(())
    }
    /// Read exact effective settings with the selected package, preserving caller
    /// configuration discovery independently of the inspected target workdir.
    pub fn inspect_config(
        &self,
        caller: &Path,
        config_root: Option<&Path>,
        profile: Option<&str>,
        workdir: &Path,
    ) -> Result<Vec<u8>, String> {
        if !workdir.is_absolute() || !workdir.is_dir() {
            return Err("revust.prepare: actual target workdir is unavailable".into());
        }
        let mut args: Vec<OsString> = vec![
            "config".into(),
            "--effective".into(),
            "--workdir".into(),
            workdir.as_os_str().into(),
        ];
        if let Some(root) = config_root {
            if !root.is_absolute() {
                return Err("revust.prepare: explicit config root must be absolute".into());
            }
            args.extend(["--config-dir".into(), root.as_os_str().into()]);
        }
        if let Some(profile) = profile {
            if profile.is_empty()
                || profile.len() > 128
                || !profile
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            {
                return Err("revust.prepare: explicit profile identifier invalid".into());
            }
            args.extend(["--profile".into(), profile.into()]);
        }
        let output = self.preparation(caller, &args)?;
        let value = super::super::json::read(&output)?;
        if value["reference_revision"] != "revust-contract-v1" {
            return Err("revust.prepare: unsupported installed effective-config contract".into());
        }
        Ok(output)
    }
    /// Resolve the precise execution flags without starting a review. Raw source
    /// inspection remains independent of per-run presentation preferences.
    pub fn inspect_launcher_config(
        &self,
        snapshot: &RevustConfigSnapshot,
        keep_open: bool,
        workdir: &Path,
    ) -> Result<Vec<u8>, String> {
        snapshot.verify()?;
        if !workdir.is_absolute() || !workdir.is_dir() {
            return Err("revust.prepare: actual target workdir is unavailable".into());
        }
        let mut args: Vec<OsString> = vec![
            "config".into(),
            "--effective".into(),
            "--workdir".into(),
            workdir.as_os_str().into(),
        ];
        args.extend(snapshot.launcher_args(keep_open));
        let result = self.preparation(&snapshot.directory, &args)?;
        snapshot.verify()?;
        Ok(result)
    }
    /// Inspect only saved evidence with the selected installed reader. No current
    /// profile/account is loaded and no review/provider may be started by view.
    pub fn inspect_archive(&self, archive: &Path) -> Result<serde_json::Value, String> {
        if !archive.is_absolute() || !archive.is_dir() {
            return Err("revust.archive: actual saved archive unavailable".into());
        }
        let output = self.preparation(
            archive,
            &[
                "view".into(),
                "--archive".into(),
                archive.as_os_str().into(),
                "--no-tui".into(),
            ],
        )?;
        super::super::json::read(&output)
    }
}
