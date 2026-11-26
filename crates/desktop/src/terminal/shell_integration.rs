//! Launch-only integration; startup files are read, never rewritten.
use portable_pty::CommandBuilder;
use std::{io, path::Path};
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}
pub(crate) fn prepare(command: &mut CommandBuilder) -> io::Result<Option<tempfile::TempDir>> {
    let shell = command
        .get_argv()
        .first()
        .and_then(|p| Path::new(p).file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if !matches!(shell, "zsh" | "bash" | "fish") {
        return Ok(None);
    }
    let shell = shell.to_owned();
    let dir = tempfile::Builder::new().prefix("guild-shell-").tempdir()?;
    match shell.as_str() {
        "zsh" => {
            std::fs::write(
                dir.path().join("integration.zsh"),
                include_str!("../../assets/shell/integration.zsh"),
            )?;
            if let Some(original) = command
                .get_env("ZDOTDIR")
                .map(std::ffi::OsStr::to_os_string)
            {
                command.env("GUILD_SHELL_ZDOTDIR", original);
                command.env("GUILD_SHELL_ZDOTDIR_SET", "1");
            }
            command.env("GUILD_SHELL_BRIDGE", dir.path());
            for stage in [".zshenv", ".zprofile", ".zshrc", ".zlogin"] {
                let finish = if stage == ".zlogin" {
                    "source \"$GUILD_SHELL_BRIDGE/integration.zsh\"\nunset GUILD_SHELL_BRIDGE GUILD_SHELL_ZDOTDIR GUILD_SHELL_ZDOTDIR_SET\n"
                } else {
                    "if (( ${+ZDOTDIR} )); then GUILD_SHELL_ZDOTDIR=$ZDOTDIR; GUILD_SHELL_ZDOTDIR_SET=1; else unset GUILD_SHELL_ZDOTDIR_SET; fi\nZDOTDIR=$GUILD_SHELL_BRIDGE\n"
                };
                let wrapper = format!(
                    "if [[ ${{GUILD_SHELL_ZDOTDIR_SET-}} ]]; then ZDOTDIR=$GUILD_SHELL_ZDOTDIR; else unset ZDOTDIR; fi\n[[ -r ${{ZDOTDIR:-$HOME}}/{stage} ]] && source \"${{ZDOTDIR:-$HOME}}/{stage}\"\n{finish}"
                );
                std::fs::write(dir.path().join(stage), wrapper)?;
            }
            command.env("ZDOTDIR", dir.path());
        }
        "bash" => {
            let script = dir.path().join("integration.bash");
            std::fs::write(&script, include_str!("../../assets/shell/integration.bash"))?;
            if let Some(old) = command
                .get_env("PROMPT_COMMAND")
                .map(std::ffi::OsStr::to_os_string)
            {
                command.env("GUILD_SHELL_OLD_PROMPT", old);
            }
            command.env("GUILD_SHELL_BOOTSTRAP", &script);
            let bootstrap = format!("source {} ; _guild_precmd", quote(&script));
            let old = command
                .get_env("PROMPT_COMMAND")
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            command.env("GUILD_SHELL_BOOTSTRAP_COMMAND", &bootstrap);
            command.env(
                "PROMPT_COMMAND",
                format!(
                    "{bootstrap}{}",
                    if old.is_empty() {
                        String::new()
                    } else {
                        format!("; {old}")
                    }
                ),
            );
        }
        "fish" => {
            let script = dir.path().join("integration.fish");
            std::fs::write(&script, include_str!("../../assets/shell/integration.fish"))?;
            command.arg("-C");
            command.arg(format!("source {}", quote(&script)));
        }
        _ => unreachable!(),
    }
    Ok(Some(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::emulator::{Emulator, GridSize};
    #[test]
    fn scoped_zsh_preserves_user_startup_and_tracks_cwd_and_exit() {
        let home = tempfile::tempdir().unwrap();
        let first = home.path().join("first");
        let second = home.path().join("second dir");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let rc = r#"print -r -- STARTUP-SENTINEL
precmd_functions+=(user_precmd)
user_precmd() { print -r -- $? >> "$HOME/statuses"; print -r -- USER-PROMPT; }
"#;
        std::fs::write(home.path().join(".zshrc"), rc).unwrap();
        let mut command = CommandBuilder::new("/bin/zsh");
        command.args(["-l", "-i"]);
        command.env("HOME", home.path());
        command.env_remove("ZDOTDIR");
        let material = prepare(&mut command).unwrap();
        let mut emulator = Emulator::spawn(command, &first, GridSize::new(100, 24)).unwrap();
        emulator.hold_shell_material(material);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while emulator.commands().is_empty() {
            assert!(
                std::time::Instant::now() < until,
                "no prompt metadata: {}",
                emulator.failure_tail()
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(emulator.failure_tail().contains("STARTUP-SENTINEL"));
        let line = format!(
            "print -r -- OLD-OUTPUT; cd {}; print -r -- NEW-OUTPUT; false\r",
            quote(&second)
        );
        assert!(emulator.send(line.as_bytes()));
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if emulator.live_cwd().as_deref() == Some(second.as_path())
                && emulator.commands().iter().any(|c| c.status == Some(1))
            {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "shell hooks did not publish cwd/status: {}",
                emulator.failure_tail()
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(emulator.failure_tail().contains("USER-PROMPT"));
        assert_eq!(
            std::fs::read_to_string(home.path().join("statuses"))
                .unwrap()
                .lines()
                .last(),
            Some("1"),
            "user prompt hook lost the command status"
        );
        let _complete = emulator
            .commands()
            .into_iter()
            .find(|c| c.status == Some(1))
            .unwrap();
        assert!(emulator.navigate_command(true));
        assert_eq!(emulator.command_context(None).3, Some(1));
        assert!(emulator.select_command_output(None));
        let selected = emulator.selected_text().unwrap();
        assert!(selected.contains("OLD-OUTPUT") && selected.contains("NEW-OUTPUT"));
        assert!(
            !selected.contains("USER-PROMPT") && !selected.contains("print -r --"),
            "selected prompt instead of command output: {selected}"
        );
        assert_eq!(
            std::fs::read_to_string(home.path().join(".zshrc")).unwrap(),
            rc
        );
        emulator.kill();
    }
    #[test]
    fn scoped_bash_preserves_login_profile_and_prompt_hook() {
        let home = tempfile::tempdir().unwrap();
        let second = home.path().join("second");
        std::fs::create_dir(&second).unwrap();
        let profile = r#"printf 'BASH-STARTUP\n'
user_prompt() { printf 'USER-PROMPT\n'; printf x >> "$HOME/hook-ran"; }
PROMPT_COMMAND="${PROMPT_COMMAND}; user_prompt"
"#;
        std::fs::write(home.path().join(".bash_profile"), profile).unwrap();
        let mut command = CommandBuilder::new("/bin/bash");
        command.args(["-l", "-i"]);
        command.env("HOME", home.path());
        let material = prepare(&mut command).unwrap();
        let mut emulator = Emulator::spawn(command, home.path(), GridSize::new(100, 24)).unwrap();
        emulator.hold_shell_material(material);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while emulator.commands().is_empty() {
            assert!(
                std::time::Instant::now() < until,
                "no Bash boundaries: {}",
                emulator.failure_tail()
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            emulator.send(
                format!("cd {}; printf 'BASH-OUTPUT\\n'; false\r", quote(&second)).as_bytes()
            )
        );
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if emulator.live_cwd().as_deref() == Some(second.as_path())
                && emulator.commands().iter().any(|c| c.status == Some(1))
            {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "Bash cwd/status missing: {}",
                emulator.failure_tail()
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let text = emulator.failure_tail();
        assert!(text.contains("BASH-STARTUP") && text.contains("USER-PROMPT"));
        assert!(
            std::fs::read(home.path().join("hook-ran")).unwrap().len() >= 2,
            "user prompt hook was lost after bootstrap"
        );
        assert_eq!(
            std::fs::read_to_string(home.path().join(".bash_profile")).unwrap(),
            profile
        );
        emulator.kill();
    }
}
