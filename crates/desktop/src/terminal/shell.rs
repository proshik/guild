//! Which shell to run in a terminal tab.
//!
//! Not `sh` and not "whatever turns up": a person has configured a shell,
//! and the tab has to be the same one as in their terminal — otherwise
//! neither aliases, nor completions, nor the prompt they are used to
//! work.

use portable_pty::CommandBuilder;

/// The fallback shell, when there is nothing to learn from.
///
/// `/bin/sh` exists everywhere. It is certainly not the shell the person
/// is used to, but a tab that failed to open is worse than an unfamiliar
/// one.
const FALLBACK: &str = "/bin/sh";

/// The command that starts the shell.
///
/// Taken from `SHELL` — that is where the person's choice, recorded by the
/// system at login, lives.
pub fn command() -> CommandBuilder {
    build(std::env::var("SHELL").ok().as_deref())
}

/// A plain dock shell must not inherit the builder or an agent caller identity.
pub fn panel_command() -> CommandBuilder {
    let mut command = command();
    clear_panel_identity(&mut command);
    command
}

fn clear_panel_identity(command: &mut CommandBuilder) {
    for key in [
        guild_session::SESSION_VAR,
        guild_session::SOCKET_VAR,
        guild_session::HOOK_SOCKET_VAR,
    ] {
        command.env_remove(key);
    }
}

/// Match the supported prompt to this terminal without changing shell files.
/// Other prompts ignore these variables; explicit assignments in .zshrc win.
/// This applies at launch: an existing shell keeps its initialized prompt.
pub fn align_theme(command: &mut CommandBuilder, theme: crate::theme::Kind) {
    if theme.is_light() {
        command.env("SOLARIZED_THEME", "light");
        for key in [
            "AGNOSTER_CONTEXT_BG",
            "AGNOSTER_STATUS_BG",
            "AGNOSTER_DIR_BG",
        ] {
            command.env(key, "default");
        }
        for key in ["AGNOSTER_CONTEXT_FG", "AGNOSTER_STATUS_FG"] {
            command.env(key, "default");
        }
        command.env("AGNOSTER_DIR_FG", "blue");
    }
}

/// Which shell to run, from a value rather than the environment.
///
/// **One rule, two callers.** A terminal tab wants a shell; a run of
/// M8.4 wants a shell *and* a line to give it. Choosing the program
/// twice is how the tab and the Output panel would come to run in
/// different shells.
pub(crate) fn program(shell: Option<&str>) -> &str {
    match shell {
        Some(path) if !path.trim().is_empty() => path,
        _ => FALLBACK,
    }
}

/// The same logic with an explicit value: that way it can be checked
/// without touching the process environment (modifying it is `unsafe` in
/// Rust 2024, and tests are barred from it for good reason — they run in
/// threads).
fn build(shell: Option<&str>) -> CommandBuilder {
    let mut command = CommandBuilder::new(program(shell));
    // A login shell: on macOS the `PATH` is assembled at login —
    // `/etc/paths`, `path_helper`, the user's profile. Without `-l` the
    // tab gets a stripped-down `PATH` and cannot find what the person
    // installed themselves.
    command.arg("-l");
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(command: &CommandBuilder) -> Vec<String> {
        command
            .get_argv()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    /// The tab must open the shell the person chose: otherwise none of
    /// their configuration works in it.
    #[test]
    fn the_shell_of_the_person_is_used() {
        let command = build(Some("/opt/homebrew/bin/fish"));

        assert_eq!(argv(&command)[0], "/opt/homebrew/bin/fish");
    }

    /// A login shell: on macOS the `PATH` is assembled at login, and
    /// without it the tab cannot find what the person installed
    /// themselves.
    #[test]
    fn it_is_a_login_shell() {
        assert_eq!(argv(&build(Some("/bin/zsh"))), vec!["/bin/zsh", "-l"]);
    }

    /// `SHELL` may be unset — that happens in a stripped-down
    /// environment. A tab that failed to open is worse than an unfamiliar
    /// one.
    #[test]
    fn without_a_setting_a_shell_that_exists_everywhere_is_used() {
        assert_eq!(argv(&build(None))[0], FALLBACK);
    }

    /// An empty variable also means "unset" rather than "a shell with an
    /// empty name": running such a command would fail.
    #[test]
    fn an_empty_setting_counts_as_absent() {
        assert_eq!(argv(&build(Some("   ")))[0], FALLBACK);
        assert_eq!(argv(&build(Some("")))[0], FALLBACK);
    }
}
