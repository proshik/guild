use guild_desktop::run::source;
use std::{os::unix::fs::PermissionsExt, path::Path};

/// All four runner argv shapes traverse the actual login-shell quoting boundary.
/// The local stand-in prints argv; it never evaluates project commands/templates.
#[test]
fn four_runner_invocations_keep_names_and_parameters_as_data() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("runner with spaces");
    std::fs::write(&program, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let value = "a ' quote; $(touch NEVER)";
    for (filename, text, args) in [
        (
            "Justfile",
            "run who:\n    echo {{who}}\n",
            vec![value.to_owned()],
        ),
        (
            "GNUmakefile",
            "first second:\n\techo ok\n",
            vec![format!("WHO={value}")],
        ),
        (
            "package.json",
            r#"{"scripts":{"check;$(touch NEVER)":"echo ok"}}"#,
            vec![],
        ),
        (
            "Taskfile.yaml",
            "version: '3'\ntasks:\n  verify:\n    cmds: [echo ok]\n",
            vec![format!("WHO={value}")],
        ),
    ] {
        let task = source::parse(Path::new(filename), text).remove(0);
        let mut argv = task.argv(&args, Some("npm")).unwrap();
        argv[0] = program.to_string_lossy().into_owned();
        let result = std::process::Command::new("/bin/sh")
            .args(["-l", "-c", &source::shell_line(&argv)])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(result.status.success());
        assert_eq!(
            String::from_utf8(result.stdout)
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            argv[1..]
        );
    }
    assert!(!dir.path().join("NEVER").exists());
}
