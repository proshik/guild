//! Owner-invoked manual recovery, available inside the distributed app without a checkout.
use guild_session::update::transaction::{self, Failure};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
pub(super) fn recover(
    state: PathBuf,
    id: uuid::Uuid,
    choice: transaction::Recovery,
) -> Result<(), Failure> {
    let cancel = AtomicBool::new(false);
    let store = transaction::Store::new(state);
    let receipt = store.load(id)?;
    let slots = [
        receipt.destination().to_path_buf(),
        receipt.previous_path(),
        receipt.prepared_path(),
        receipt.failed_path(),
    ];
    let executables: Vec<_> = slots
        .iter()
        .map(|app| app.join("Contents/MacOS/guild-desktop"))
        .collect();
    let actual = std::env::current_exe()
        .map_err(|_| Failure::Changed)?
        .canonicalize()
        .map_err(|_| Failure::Changed)?;
    receipt.verify_recovery_executor(&actual, &cancel)?;
    if crate::system::update_processes::in_use(&executables).map_err(|_| Failure::Conflict)? {
        return Err(Failure::Conflict);
    }
    let receipt = store.recover(id, choice, &cancel)?;
    println!(
        "Update recovery completed: {}",
        receipt.destination().display()
    );
    Ok(())
}
/// A shell-safe owner command, displayed only for a journal already loaded off-thread.
pub fn command(
    executable: &Path,
    state: &Path,
    id: uuid::Uuid,
    choice: transaction::Recovery,
) -> String {
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
    }
    let action = match choice {
        transaction::Recovery::RestorePrevious => "restore-previous",
        transaction::Recovery::KeepCurrent => "keep-current",
    };
    format!(
        "{} --guild-update-recover {} {} {action} --apps-closed",
        quote(executable),
        quote(state),
        id
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn recovery_command_preserves_literal_paths_without_shell_interpolation() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("owned ' $(echo literal) executable");
        std::fs::write(&executable, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let state = root.path().join("state ' $(echo literal)");
        let id = uuid::Uuid::new_v4();
        let output = std::process::Command::new("/bin/sh")
            .args([
                "-c",
                &command(
                    &executable,
                    &state,
                    id,
                    transaction::Recovery::RestorePrevious,
                ),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            [
                "--guild-update-recover",
                state.to_str().unwrap(),
                &id.to_string(),
                "restore-previous",
                "--apps-closed"
            ]
        );
    }
}
