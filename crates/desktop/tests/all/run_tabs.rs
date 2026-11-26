use guild_desktop::run::tabs::{Invocation, RunTabs};

fn invocation(line: &str, cwd: &str) -> Invocation {
    Invocation {
        line: line.into(),
        cwd: cwd.into(),
        task: None,
    }
}
#[test]
fn invocation_identity_retains_distinct_arguments_and_directories() {
    let mut tabs = RunTabs::default();
    let first = tabs.insert(invocation("test one", "/project/a"), "first output");
    let second = tabs.insert(invocation("test two", "/project/a"), "second output");
    let third = tabs.insert(invocation("test one", "/project/b"), "other directory");
    assert_eq!(tabs.entries().len(), 3);
    assert_eq!(
        tabs.insert(invocation("test one", "/project/a"), "rerun output"),
        first
    );
    assert_eq!(tabs.active().unwrap().id, first);
    assert_eq!(tabs.get(second).unwrap().value, "second output");
    tabs.select(second);
    tabs.remove(first);
    assert_eq!(tabs.active().unwrap().id, second);
    tabs.remove(second);
    assert_eq!(tabs.active().unwrap().id, third);
    tabs.select(first);
    assert_eq!(tabs.active().unwrap().id, third);
    assert!(
        RunTabs::<String>::default().entries().is_empty(),
        "restart has no invented output or invocations"
    );
}

#[test]
fn source_titles_capture_each_runner_and_manager_without_merging_variants() {
    use guild_desktop::run::source::{Identity, Runner, Task};
    for (runner, manager, name, expected) in [
        (Runner::Just, None, "greet", "just greet"),
        (Runner::Make, None, "serve", "make serve"),
        (Runner::Make, None, "", "make"),
        (Runner::Task, None, "default", "task default"),
        (Runner::Package, Some("npm"), "check", "npm check"),
        (Runner::Package, Some("pnpm"), "check", "pnpm check"),
        (Runner::Package, Some("yarn"), "check", "yarn check"),
        (Runner::Package, Some("bun"), "check", "bun check"),
    ] {
        let source = match runner {
            Runner::Just => "nested/justfile",
            Runner::Make => "nested/Makefile",
            Runner::Task => "nested/Taskfile.yml",
            Runner::Package => "nested/package.json",
        };
        let task = Task {
            identity: Identity {
                source: source.into(),
                runner,
                name: name.into(),
            },
            offset: 0,
            row: 0,
            parameters: vec![],
            managers: vec![],
            default: false,
        };
        let args = if matches!(runner, Runner::Make | Runner::Task) {
            "WHO=O'Brien 世界"
        } else {
            "O'Brien 世界"
        };
        let captured = Invocation {
            cwd: "/project/nested".into(),
            line: task.command(&[args.into()], manager).unwrap(),
            task: Some(task.identity.clone()),
        };
        assert_eq!(captured.title(), expected);
        assert!(captured.details().contains(source));
        assert!(captured.details().contains("/project/nested"));
        assert!(captured.details().contains(&captured.line));
        let mut variant = captured.clone();
        variant.line = task.command(&[], manager).unwrap();
        let mut tabs = RunTabs::default();
        let first = tabs.insert(captured.clone(), ());
        let second = tabs.insert(variant.clone(), ());
        assert_ne!(first, second, "short titles do not merge argument variants");
        assert_eq!(captured.title(), variant.title());
        assert_ne!(captured.details(), variant.details());
        assert_eq!(tabs.insert(captured, ()), first);
    }
    let typed = invocation("cargo test -p guild-desktop -- --nocapture", "/project");
    assert_eq!(typed.title(), typed.line);
}
