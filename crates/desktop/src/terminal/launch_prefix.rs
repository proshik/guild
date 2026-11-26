//! Terminal-only shell wrappers. Arguments and context cross the shell as data.
use portable_pty::CommandBuilder;
use std::path::Path;

#[cfg(test)]
fn wrap(
    command: CommandBuilder,
    prefix: &str,
    cwd: &Path,
    shell: &str,
) -> anyhow::Result<CommandBuilder> {
    wrap_policy(command, prefix, cwd, shell, &Vec::new())
}
pub(super) fn wrap_policy(
    mut command: CommandBuilder,
    prefix: &str,
    cwd: &Path,
    shell: &str,
    network: &guild_session::network::Environment,
) -> anyhow::Result<CommandBuilder> {
    if prefix.trim().is_empty() {
        return Ok(command);
    }
    match Path::new(shell).file_name().and_then(|name| name.to_str()) {
        Some("zsh" | "bash" | "sh") => {}
        _ => anyhow::bail!("Launch prefix requires zsh, bash or sh; selected shell is unsupported"),
    }
    // Explicit overrides, plus inherited Guild context, must survive rc files.
    // Values travel in private environment slots, never in process arguments.
    let mut names: std::collections::BTreeSet<String> = command
        .iter_extra_env_as_str()
        .map(|(key, _)| key.to_owned())
        .collect();
    names.extend(
        command
            .iter_full_env_as_str()
            .filter(|(key, _)| key.starts_with("GUILD_"))
            .map(|(key, _)| key.to_owned()),
    );
    names.extend(
        [
            "HOME",
            "CODEX_HOME",
            "CLAUDE_CONFIG_DIR",
            "XDG_CONFIG_HOME",
            "PATH",
            "LANG",
            "TERM",
            "COLORTERM",
            "CLAUDE_CODE_SSE_PORT",
            "OPENCODE_CONFIG_CONTENT",
            "GUILD_SESSION",
            "GUILD_RUNTIME",
            "GUILD_SOCKET",
            "GUILD_HOOK_SOCKET",
            "GUILD_IDENTITY_DIR",
            "GUILD_USER_STATUSLINE",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    names.extend(network.iter().map(|(k, _)| (*k).to_owned()));
    let dropped: Vec<_> = guild_session::markers_to_drop()
        .chain(["TERM_PROGRAM", "TERM_PROGRAM_VERSION"])
        .collect();
    let restore = r#"builtin cd -- "$1" || exit; shift; while [ "$1" != -- ]; do if [ "$2" != - ]; then eval 'guild_launch_value=${'"$2"'}'; export "$1=$guild_launch_value"; unset "$2"; else unset "$1"; fi; shift 2; done; shift; unset guild_launch_value;"#;
    let restore = if Path::new(shell).file_name().is_some_and(|n| n == "sh") {
        restore.replacen("builtin cd", "cd", 1)
    } else {
        restore.to_owned()
    };
    // Validate the external executable after restoring PATH, then remove only the
    // same-name function in this child shell. Keep
    // bare argv[0]: native harness recognition must not see a path with spaces.
    let resolve = r#"guild_launch_program=; case "$1" in /*) guild_launch_program=$1;; */*) guild_launch_program="$PWD/$1";; *) guild_launch_path=$PATH; while :; do guild_launch_dir=${guild_launch_path%%:*}; case "$guild_launch_path" in *:*) guild_launch_path=${guild_launch_path#*:}; guild_launch_more=1;; *) guild_launch_more=0;; esac; [ -n "$guild_launch_dir" ] || guild_launch_dir=.; if [ -f "$guild_launch_dir/$1" ] && [ -x "$guild_launch_dir/$1" ]; then case "$guild_launch_dir" in /*) guild_launch_program="$guild_launch_dir/$1";; *) guild_launch_program="$PWD/$guild_launch_dir/$1";; esac; break; fi; [ "$guild_launch_more" = 1 ] || break; done;; esac; if [ ! -f "$guild_launch_program" ] || [ ! -x "$guild_launch_program" ]; then printf 'Guild: harness executable not found: %s\n' "$1" >&2; exit 127; fi;"#;
    // zsh diagnoses removal of an absent function; bash and POSIX sh treat it
    // as a no-op. Generate zsh syntax only for zsh, never for another shell.
    let remove_function = if Path::new(shell).file_name().is_some_and(|n| n == "zsh") {
        r#"case "$1" in */*) ;; *) if (( ${+functions[$1]} )); then unset -f "$1"; fi;; esac;"#
    } else {
        r#"case "$1" in */*) ;; *) unset -f "$1";; esac;"#
    };
    let script = format!(
        "{restore}\n{resolve}\n{remove_function}\n{prefix} \"$@\"\nguild_prefix_status=$?; if [ \"$guild_prefix_status\" -ne 0 ]; then printf 'Guild: wrapped command exited with status %s\\n' \"$guild_prefix_status\" >&2; fi; exit \"$guild_prefix_status\""
    );
    let original = command.get_argv().clone();
    let mut argv = vec![
        shell.into(),
        "-l".into(),
        "-i".into(),
        "-c".into(),
        script.into(),
    ];
    argv.push("guild-launch-prefix".into());
    argv.push(cwd.as_os_str().to_owned());
    static NEXT_SNAPSHOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // Allocate a namespace absent from the inherited environment, including
    // explicitly overridden keys. Existing user variables remain untouched.
    let namespace = loop {
        let serial = NEXT_SNAPSHOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let candidate = format!("GUILD_LAUNCH_SNAPSHOT_{}_{}_", std::process::id(), serial);
        if !command
            .iter_full_env_as_str()
            .any(|(key, _)| key.starts_with(&candidate))
        {
            break candidate;
        }
    };
    for (index, key) in names.into_iter().enumerate() {
        argv.push(key.clone().into());
        if network
            .iter()
            .any(|(name, value)| *name == key && value.is_none())
        {
            argv.push("-".into());
            continue;
        }
        if let Some(value) = command
            .get_env(&key)
            .filter(|_| !dropped.contains(&key.as_str()))
            .map(std::ffi::OsStr::to_owned)
        {
            let snapshot = format!("{namespace}{index}");
            command.env(&snapshot, value);
            argv.push(snapshot.into());
        } else {
            argv.push("-".into());
        }
    }
    for key in dropped {
        argv.extend([key.into(), "-".into()]);
    }
    argv.push("--".into());
    argv.extend(original);
    *command.get_argv_mut() = argv;
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn supported_shells_bypass_functions_and_preserve_literal_argv_and_failure() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh"] {
            for program in ["claude", "codex", "opencode"] {
                for function_present in [false, true] {
                    let fixture = tempfile::tempdir().unwrap();
                    let dir = fixture.path();
                    let observed = dir.join("argv");
                    let prefix_observed = dir.join("prefix-argv");
                    let fake = dir.join(program);
                    std::fs::write(&fake, "#!/bin/sh\nprintf '%s\\0' \"$0\" \"$@\" >> \"$OBSERVED\"\nprintf 'fixture-child-stderr\\n' >&2\nprintf '%s' \"$FIXTURE_RC\"\nexit 23\n").unwrap();
                    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755))
                        .unwrap();
                    let mut rc = String::from(
                        "export FIXTURE_RC=loaded\ndp() { printf '%s\\0' \"$1\" \"$2\" \"$3\" > \"$PREFIX_OBSERVED\"; shift 2; \"$@\"; }\n",
                    );
                    if function_present {
                        rc.push_str(&format!(
                            "{program}() {{ printf intercepted >&2; return 79; }}\n"
                        ));
                    }
                    for name in [".zshrc", ".bash_profile", "profile"] {
                        std::fs::write(dir.join(name), &rc).unwrap();
                    }
                    let words = [
                        "--model",
                        "configured 'single' \"double\"",
                        "resume",
                        "conversation with spaces",
                        "--no-daemon",
                        "--",
                        "Guild 'single' \"double\" $(/usr/bin/touch sentinel) Привет",
                        "",
                    ];
                    let mut command = CommandBuilder::new(program);
                    command.env_clear();
                    command.args(words);
                    command.env("HOME", dir);
                    command.env("ZDOTDIR", dir);
                    command.env("ENV", dir.join("profile"));
                    command.env("PATH", dir);
                    command.env("OBSERVED", &observed);
                    command.env("PREFIX_OBSERVED", &prefix_observed);
                    let output = run(wrap(
                        command,
                        "dp 'prefix with spaces' 'quotes \" and $(/usr/bin/touch sentinel)'",
                        dir,
                        shell,
                    )
                    .unwrap());
                    let context =
                        format!("{shell} {program} function={function_present}: {output:?}");
                    assert_eq!(output.status.code(), Some(23), "{context}");
                    assert_eq!(output.stdout, b"loaded", "{context}");
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    assert!(!stderr.contains("unset:"), "{context}");
                    assert!(!stderr.contains("intercepted"), "{context}");
                    assert_eq!(
                        stderr.matches("fixture-child-stderr").count(),
                        1,
                        "{context}"
                    );
                    assert!(
                        stderr.contains("Guild: wrapped command exited with status 23"),
                        "{context}"
                    );
                    let mut expected = Vec::new();
                    for word in std::iter::once(fake.to_str().unwrap()).chain(words) {
                        expected.extend_from_slice(word.as_bytes());
                        expected.push(0);
                    }
                    assert_eq!(std::fs::read(&observed).unwrap(), expected, "{context}");
                    assert_eq!(std::fs::read(&prefix_observed).unwrap(), format!("prefix with spaces\0quotes \" and $(/usr/bin/touch sentinel)\0{program}\0").as_bytes(), "{context}");
                    assert!(!dir.join("sentinel").exists(), "{context}");
                }
            }
        }
    }

    #[test]
    fn missing_bare_harness_retains_diagnostic_in_supported_shells() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh"] {
            let fixture = tempfile::tempdir().unwrap();
            let mut command = CommandBuilder::new("guild_fixture_missing_harness");
            command.env_clear();
            command.env("HOME", fixture.path());
            command.env("ZDOTDIR", fixture.path());
            command.env("PATH", fixture.path());
            let output = run(wrap(command, "false", fixture.path(), shell).unwrap());
            assert_eq!(output.status.code(), Some(127), "{shell}: {output:?}");
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("Guild: harness executable not found: guild_fixture_missing_harness")
            );
        }
    }

    #[test]
    fn zsh_function_runs_once_with_literal_arguments_and_restored_context() {
        let fixture = tempfile::tempdir().unwrap();
        let dir = fixture.path();
        let output = dir.join("observed");
        let fake = dir.join("codex");
        std::fs::write(&fake, "#!/bin/sh\nprintf '%s\\n' \"$PWD\" \"$GUILD_SESSION\" \"$WRAPPED\" \"$@\" > \"$OBSERVED\"\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.join(".zshrc"), "cd /\nexport PATH=/bad GUILD_SESSION=bad\ndp() { export WRAPPED=once; \"$@\"; }\ncodex() { return 77; }\nalias codex='dp codex'\n").unwrap();
        let mut command = CommandBuilder::new("codex");
        let prompt = "literal ' quote ; $(touch never) Привет";
        command.args(["resume", "conversation", "--no-daemon", "--", prompt]);
        command.env("HOME", dir);
        command.env("ZDOTDIR", dir);
        command.env("PATH", dir);
        command.env("GUILD_SESSION", "owned");
        command.env("OBSERVED", &output);
        let wrapped = wrap(command, "dp", dir, "/bin/zsh").unwrap();
        let mut child = std::process::Command::new(&wrapped.get_argv()[0]);
        child.args(&wrapped.get_argv()[1..]).current_dir(dir);
        child.envs(wrapped.iter_full_env_as_str());
        let result = child.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let observed = std::fs::read_to_string(output).unwrap();
        assert_eq!(
            observed,
            format!(
                "{}\nowned\nonce\nresume\nconversation\n--no-daemon\n--\n{prompt}\n",
                dir.display()
            )
        );
        assert!(!dir.join("never").exists());
    }
    #[test]
    fn rc_overrides_cannot_replace_hooks_discovery_identity_or_locale() {
        let fixture = tempfile::tempdir().unwrap();
        let dir = fixture.path();
        let entries = [
            ("PATH", "/usr/bin:/bin"),
            ("LANG", "en_US.UTF-8"),
            ("TERM", "xterm-256color"),
            ("COLORTERM", "truecolor"),
            ("GUILD_SESSION", "owned"),
            ("GUILD_SOCKET", "/fixture/socket ' with space"),
            ("GUILD_HOOK_SOCKET", "/fixture/hook"),
            ("GUILD_IDENTITY_DIR", "/fixture/identity"),
            ("GUILD_USER_STATUSLINE", "printf 'fixture'"),
            ("CLAUDE_CODE_SSE_PORT", "1234"),
            ("OPENCODE_CONFIG_CONTENT", "{\"fixture\":\"$()\"}\n"),
        ];
        let mut rc =
            String::from("cd /\ndp() { \"$@\"; }\nexport CLAUDECODE=bad TERM_PROGRAM=bad\n");
        let mut script = String::from("printf '%s\\0'");
        let mut expected = Vec::new();
        let mut command = CommandBuilder::new("/bin/sh");
        command.env_clear();
        for (key, value) in entries {
            rc.push_str(&format!("export {key}=bad\n"));
            script.push_str(&format!(" \"${key}\""));
            expected.extend_from_slice(value.as_bytes());
            expected.push(0);
            command.env(key, value);
        }
        script.push_str("; test -z \"${CLAUDECODE+x}${TERM_PROGRAM+x}\"");
        command.args(["-c", &script]);
        command.env("HOME", dir);
        command.env("ZDOTDIR", dir);
        std::fs::write(dir.join(".zshrc"), rc).unwrap();
        let output = run(wrap(command, "dp", dir, "/bin/zsh").unwrap());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected);
    }

    #[test]
    fn startup_restores_integration_directories_and_their_absence_without_argv_values() {
        let fixture = tempfile::tempdir().unwrap();
        let dir = fixture.path();
        std::fs::write(dir.join(".zshrc"), "export HOME=/wrong CODEX_HOME=/wrong CLAUDE_CONFIG_DIR=/wrong XDG_CONFIG_HOME=/wrong\ndp() { \"$@\"; }\n").unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "printf '%s\\n' \"$HOME\" \"$CODEX_HOME\" \"${CLAUDE_CONFIG_DIR-unset}\" \"${XDG_CONFIG_HOME-unset}\" \"$OPENCODE_CONFIG_CONTENT\"; env | /usr/bin/grep '^GUILD_LAUNCH_SNAPSHOT_' || :"]);
        command.env_clear();
        command.env("HOME", dir);
        command.env("ZDOTDIR", dir);
        command.env("PATH", "/bin:/usr/bin");
        command.env("CODEX_HOME", "/fixture/codex");
        command.env("OPENCODE_CONFIG_CONTENT", "private-fixture-token");
        let wrapped = wrap(command, "dp", dir, "/bin/zsh").unwrap();
        let argv = wrapped.get_argv().clone();
        let output = run(wrapped);
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!(
                "{}\n/fixture/codex\nunset\nunset\nprivate-fixture-token\n",
                dir.display()
            )
        );
        assert!(
            !argv
                .iter()
                .any(|arg| arg.to_string_lossy().contains("private-fixture-token")),
            "restored values must never enter the wrapper argv"
        );
    }

    #[test]
    fn inherited_integration_directories_survive_startup() {
        const CHILD: &str = "GUILD_PREFIX_INHERITED_FIXTURE";
        if std::env::var_os(CHILD).is_some() {
            let home = std::env::var_os("HOME").unwrap();
            let mut command = CommandBuilder::new("/bin/sh");
            command.args(["-c", "printf '%s\\n' \"$HOME\" \"$CODEX_HOME\" \"$CLAUDE_CONFIG_DIR\" \"$XDG_CONFIG_HOME\" \"$HTTPS_PROXY\""]);
            let output = run(wrap(command, "dp", Path::new(&home), "/bin/zsh").unwrap());
            assert!(output.status.success(), "{:?}", output);
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                format!(
                    "{}\n/fixture/codex\n/fixture/claude\n/fixture/xdg\nfixture-proxy\n",
                    Path::new(&home).display()
                )
            );
            return;
        }
        let fixture = tempfile::tempdir().unwrap();
        std::fs::write(fixture.path().join(".zshrc"), "export HOME=/wrong CODEX_HOME=/wrong CLAUDE_CONFIG_DIR=/wrong XDG_CONFIG_HOME=/wrong\ndp() { export HTTPS_PROXY=fixture-proxy; \"$@\"; }\n").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "terminal::launch_prefix::tests::inherited_integration_directories_survive_startup",
                "--nocapture",
            ])
            .env_clear()
            .env("PATH", "/bin:/usr/bin")
            .env("HOME", fixture.path())
            .env("ZDOTDIR", fixture.path())
            .env(CHILD, "1")
            .env("CODEX_HOME", "/fixture/codex")
            .env("CLAUDE_CONFIG_DIR", "/fixture/claude")
            .env("XDG_CONFIG_HOME", "/fixture/xdg")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn successfully_started_harness_nonzero_has_neutral_diagnostic() {
        let fixture = tempfile::tempdir().unwrap();
        let dir = fixture.path();
        std::fs::write(dir.join(".zshrc"), "dp() { \"$@\"; }\n").unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "echo harness-started; exit 17"]);
        command.env("HOME", dir);
        command.env("ZDOTDIR", dir);
        let output = run(wrap(command, "dp", dir, "/bin/zsh").unwrap());
        assert_eq!(output.status.code(), Some(17));
        assert_eq!(output.stdout, b"harness-started\n");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("Guild: wrapped command exited with status 17")
        );
    }

    fn run(command: CommandBuilder) -> std::process::Output {
        let mut process = std::process::Command::new(&command.get_argv()[0]);
        process.args(&command.get_argv()[1..]);
        process.env_clear().envs(command.iter_full_env_as_str());
        process.output().unwrap()
    }

    #[test]
    fn empty_prefix_keeps_direct_argv_and_unknown_shell_has_no_fallback() {
        for shell in [
            "/bin/zsh",
            "/bin/bash",
            "/bin/sh",
            "/fixture/fish",
            "/unsupported",
        ] {
            for prefix in ["", " \t "] {
                let mut command = CommandBuilder::new("fake-codex");
                command.args(["--", "literal $argument"]);
                let before = command.get_argv().clone();
                assert_eq!(
                    wrap(command, prefix, Path::new("/"), shell)
                        .unwrap()
                        .get_argv(),
                    &before
                );
            }
        }
        for shell in ["/fixture/fish", "/bin/csh"] {
            let error = match wrap(
                CommandBuilder::new("fake-codex"),
                "dp",
                Path::new("/"),
                shell,
            ) {
                Err(error) => error,
                Ok(_) => panic!("unsupported shell must refuse a nonempty prefix"),
            };
            assert!(error.to_string().contains("unsupported"));
        }
    }

    #[test]
    fn missing_and_broken_prefixes_never_launch_directly() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh"] {
            let fixture = tempfile::tempdir().unwrap();
            for prefix in ["guild_fixture_missing_wrapper", "false"] {
                let mut command = CommandBuilder::new("/bin/sh");
                let sentinel = fixture.path().join("should-not-exist");
                command.args(["-c", "touch \"$1\"", "fixture"]);
                command.arg(&sentinel);
                command.env("HOME", fixture.path());
                command.env("ZDOTDIR", fixture.path());
                let output = run(wrap(command, prefix, fixture.path(), shell).unwrap());
                assert!(!output.status.success());
                assert!(
                    String::from_utf8_lossy(&output.stderr)
                        .contains("wrapped command exited with status")
                );
                assert!(!sentinel.exists());
            }
        }
    }

    fn dialect(shell: &str) {
        let fixture = tempfile::tempdir().unwrap();
        let dir = fixture.path();
        let rc = "cd /; export GUILD_SESSION=wrong; dp() { export WRAPPED=once; \"$@\"; }";
        std::fs::write(dir.join(".bash_profile"), rc).unwrap();
        std::fs::write(dir.join("profile"), rc).unwrap();
        std::fs::write(dir.join("codex"), "#!/bin/sh\nexec /bin/sh \"$@\"\n").unwrap();
        std::fs::set_permissions(dir.join("codex"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let mut command = CommandBuilder::new("codex");
        command.env("PATH", dir);
        command.args([
            "-c",
            "printf '%s\\n' \"$GUILD_SESSION\" \"$WRAPPED\" \"$PWD\" \"$1\"",
            "fixture",
            "'$(echo wrong); space Привет",
        ]);
        command.env("HOME", dir);
        command.env("XDG_CONFIG_HOME", dir);
        command.env("ENV", dir.join("profile"));
        command.env("GUILD_SESSION", "owned");
        let output = run(wrap(command, "dp", dir, shell).unwrap());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!(
                "owned\nonce\n{}\n'$(echo wrong); space Привет\n",
                dir.display()
            )
        );
    }

    #[test]
    fn bash_and_sh_functions_forward_literals() {
        dialect("/bin/bash");
        dialect("/bin/sh");
    }

    #[test]
    fn wrapped_child_is_owned_and_close_stops_it_without_touching_foreign_process() {
        use crate::terminal::emulator::{Emulator, GridSize};
        let fixture = tempfile::tempdir().unwrap();
        let dir = fixture.path();
        let pid_file = dir.join("pid");
        std::fs::write(dir.join(".zshrc"), "dp() { \"$@\"; :; }\n").unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "echo $$ > \"$PID_FILE\"; exec /bin/sleep 20"]);
        command.env("HOME", dir);
        command.env("ZDOTDIR", dir);
        command.env("SHELL", "/bin/zsh");
        command.env("PID_FILE", &pid_file);
        let mut emulator =
            Emulator::spawn_harness(command, dir, GridSize::new(80, 24), "dp", &Vec::new())
                .unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !pid_file.exists() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let pid: u32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_ne!(
            Some(pid),
            emulator.process_id(),
            "wrapper must remain the parent"
        );
        let mut foreign_command = std::process::Command::new("/bin/sleep");
        foreign_command.arg("20");
        let mut foreign =
            guild_session::process::ProcessGroup::spawn(&mut foreign_command).unwrap();
        emulator.hang_up_for_quit();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while unsafe { libc::kill(pid as i32, 0) } == 0 && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_ne!(
            unsafe { libc::kill(pid as i32, 0) },
            0,
            "owned harness survived close"
        );
        assert!(
            foreign.try_wait().unwrap().is_none(),
            "foreign control must survive"
        );
        foreign.stop().unwrap();
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;
    #[test]
    fn per_harness_direct_and_proxy_survive_shell_initialization() {
        use guild_session::network::{AgentRoute, Network, PROXY_KEYS};
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".zshrc"),"export HTTP_PROXY=old http_proxy=old HTTPS_PROXY=old https_proxy=old ALL_PROXY=old all_proxy=old NO_PROXY=old no_proxy=old\n").unwrap();
        let program = dir.path().join("fixture");
        std::fs::write(&program,"#!/bin/sh\nprintf '%s\\n' \"${HTTP_PROXY-unset}\" \"${http_proxy-unset}\" \"${HTTPS_PROXY-unset}\" \"${https_proxy-unset}\" \"${ALL_PROXY-unset}\" \"${all_proxy-unset}\" \"${NO_PROXY-unset}\" \"${no_proxy-unset}\"\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut n = Network {
            address: "http://127.0.0.1:8080".into(),
            ..Default::default()
        };
        for harness in guild_session::HarnessKind::ALL {
            for route in [AgentRoute::Follow, AgentRoute::Proxy, AgentRoute::Direct] {
                n.agents.insert(harness.program().into(), route);
                let environment = n.agent_environment(harness).unwrap();
                for prefix in ["", "command", "HTTP_PROXY=http://intentional:9000 command"] {
                    let mut command = CommandBuilder::new(&program);
                    command.env("HOME", dir.path());
                    command.env("ZDOTDIR", dir.path());
                    for key in PROXY_KEYS {
                        command.env(key, "old");
                    }
                    crate::network::apply_pty_environment(&mut command, &environment);
                    let wrapped =
                        wrap_policy(command, prefix, dir.path(), "/bin/zsh", &environment).unwrap();
                    let mut child = std::process::Command::new(&wrapped.get_argv()[0]);
                    child
                        .env_clear()
                        .args(&wrapped.get_argv()[1..])
                        .envs(wrapped.iter_full_env_as_str());
                    let output = child.output().unwrap();
                    assert!(output.status.success());
                    let text = String::from_utf8(output.stdout).unwrap();
                    let values: Vec<_> = text.lines().collect();
                    assert_eq!(values.len(), 8);
                    if prefix.starts_with("HTTP_PROXY=") {
                        assert_eq!(values[0], "http://intentional:9000");
                    } else {
                        assert_eq!(
                            values[0],
                            match route {
                                AgentRoute::Proxy => "http://127.0.0.1:8080",
                                AgentRoute::Direct => "unset",
                                _ => "old",
                            }
                        );
                    }
                    if route == AgentRoute::Direct {
                        assert!(values[1..].iter().all(|v| *v == "unset"));
                    }
                    if route == AgentRoute::Proxy {
                        let proxy = n.custom().unwrap();
                        assert_eq!(values[1], proxy.url());
                        assert_eq!(&values[2..4], [proxy.url(), proxy.url()]);
                        assert_eq!(&values[4..6], ["unset", "unset"]);
                        assert_eq!(&values[6..8], [proxy.no_proxy(), proxy.no_proxy()]);
                    }
                }
            }
        }
    }
    #[test]
    fn custom_restores_all_keys_after_rc_and_preserves_prefix_override() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".zshrc"),"export ALL_PROXY=old all_proxy=old HTTP_PROXY=old http_proxy=old HTTPS_PROXY=old https_proxy=old NO_PROXY=old no_proxy=old\n").unwrap();
        let program = dir.path().join("fixture");
        std::fs::write(&program,"#!/bin/sh\nprintf '%s|%s|%s|%s|%s|%s|%s|%s' \"$HTTP_PROXY\" \"$http_proxy\" \"$HTTPS_PROXY\" \"$https_proxy\" \"$NO_PROXY\" \"$no_proxy\" \"${ALL_PROXY-unset}\" \"${all_proxy-unset}\"\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let proxy = guild_session::network::Network {
            mode: guild_session::network::Mode::Custom,
            address: "http://127.0.0.1:8080".into(),
            ..Default::default()
        }
        .custom()
        .unwrap();
        for prefix in ["command", "HTTP_PROXY=http://intentional:9000 command"] {
            let mut command = CommandBuilder::new(&program);
            command.env("ZDOTDIR", dir.path());
            for (key, value) in proxy.environment() {
                if let Some(value) = value {
                    command.env(key, value);
                } else {
                    command.env_remove(key);
                }
            }
            let wrapped = wrap_policy(
                command,
                prefix,
                dir.path(),
                "/bin/zsh",
                &proxy.environment(),
            )
            .unwrap();
            let mut child = std::process::Command::new(&wrapped.get_argv()[0]);
            child.args(&wrapped.get_argv()[1..]);
            child.envs(wrapped.iter_full_env_as_str());
            let out = child.output().unwrap();
            assert!(out.status.success());
            let out = String::from_utf8(out.stdout).unwrap();
            let values: Vec<_> = out.split('|').collect();
            assert_eq!(values.len(), 8);
            assert_eq!(
                values[0],
                if prefix == "command" {
                    proxy.url()
                } else {
                    "http://intentional:9000".into()
                }
            );
            for value in &values[1..4] {
                assert_eq!(*value, proxy.url());
            }
            assert_eq!(values[4], proxy.no_proxy());
            assert_eq!(values[5], proxy.no_proxy());
            assert_eq!(&values[6..], ["unset", "unset"]);
        }
    }
}
