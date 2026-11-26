//! Mistyped desktop invocations must fail before opening windows or installing hooks.
#[test]
fn desktop_commands_explain_the_sibling_cli_and_exit_without_starting_a_window() {
    for args in [
        &["instructions", "review"][..],
        &["status", "--json"],
        &["review", "launch"],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_guild-desktop"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("accepts no CLI commands"));
        assert!(error.contains("GUILD_SOCKET"));
        assert!(error.contains("/guild"));
    }
}
