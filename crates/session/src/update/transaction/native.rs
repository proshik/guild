use super::*;
use crate::process::ProcessGroup;
use std::{
    fs,
    process::{Command, Stdio},
    time::Instant,
};
impl Platform for Native {
    fn proof(&self, path: &Path, cancel: &AtomicBool) -> Result<Proof, Failure> {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Err(Failure::Cancelled);
        }
        let metadata = fs::symlink_metadata(path).map_err(|_| Failure::Invalid)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(Failure::Changed);
        }
        let resources = path.join("Contents/Resources");
        let info_path = resources.join("build-info.json");
        let info = match fs::symlink_metadata(&info_path) {
            Ok(_) => Some(bundle::info(path).map_err(|_| Failure::Invalid)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(Failure::Invalid),
        };
        let seal = bundle::seal(path, cancel).map_err(|_| Failure::Invalid)?;
        bundle::validate(path, info.as_ref(), cancel).map_err(|_| Failure::Invalid)?;
        let commit = if let Some(info) = &info {
            info.commit.clone()
        } else {
            let bytes = bundle::read_small(&resources.join("commit"), 1024)
                .map_err(|_| Failure::Invalid)?;
            std::str::from_utf8(&bytes)
                .map_err(|_| Failure::Invalid)?
                .lines()
                .next()
                .ok_or(Failure::Invalid)?
                .trim()
                .to_owned()
        };
        if bundle::seal(path, cancel).map_err(|_| Failure::Changed)? != seal {
            return Err(Failure::Changed);
        }
        let proof = Proof {
            build_id: info.map(|info| info.build_id),
            commit,
            seal,
        };
        proof.validate()?;
        Ok(proof)
    }
    fn launch(&self, receipt: &Receipt, old: bool, state: &Path) -> Result<(), Failure> {
        let mut command = launch_command(receipt, old, state);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = ProcessGroup::spawn(&mut command).map_err(|_| Failure::Launch)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(Failure::Launch)
                    };
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => return Err(Failure::Launch),
            }
        }
    }
}
pub(super) fn launch_command(receipt: &Receipt, old: bool, state: &Path) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command.arg("-n");
    // LaunchServices must receive this instance's state explicitly, independent
    // of the environment cached by the user's login session.
    let mut state_env = std::ffi::OsString::from("GUILD_STATE_DIR=");
    state_env.push(state);
    command.arg("--env").arg(state_env);
    for (key, value) in [
        (
            "GUILD_UPDATE_TRANSACTION",
            if old {
                String::new()
            } else {
                receipt.id.to_string()
            },
        ),
        (
            "GUILD_UPDATE_NONCE",
            if old {
                String::new()
            } else {
                receipt.nonce.to_string()
            },
        ),
    ] {
        command
            .env_remove(key)
            .arg("--env")
            .arg(format!("{key}={value}"));
    }
    command.arg(&receipt.destination);
    command
}
