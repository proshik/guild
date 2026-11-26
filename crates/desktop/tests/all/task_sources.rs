use super::scratch::Scratch;
use guild_desktop::run::{
    discover,
    source::{self, Runner},
};
use std::path::Path;

fn write_source(project: &Path, file: &str, source: &str) {
    let path = project.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

#[test]
fn just_attribute_arguments_do_not_change_visibility_or_default() {
    let tasks = source::parse(
        Path::new("justfile"),
        "first:\n    echo first\n[doc: 'Use default settings on linux windows macos private']\n[group: 'private']\nsecond:\n    echo second\n[default]\nthird:\n    echo third\n[private]\nhidden:\n    echo hidden\n",
    );
    assert_eq!(
        tasks
            .iter()
            .map(|t| (t.identity.name.as_str(), t.default))
            .collect::<Vec<_>>(),
        [("first", false), ("second", false), ("third", true)]
    );
    let tasks = source::parse(
        Path::new("justfile"),
        "first:\n    echo first\n[doc: 'Use default settings']\nsecond:\n    echo second\n",
    );
    assert!(
        !tasks[1].default,
        "a doc argument never selects the file default"
    );
}

#[test]
fn chained_import_cycles_errors_and_missing_files_fail_closed_with_watch_dependencies() {
    let project = Scratch::new("guild-import-errors-");
    write_source(
        &project,
        "Makefile",
        "include common.mk\nroot:\n\t@echo root\n",
    );
    write_source(&project, "nested/Makefile", "child:\n\t@echo child\n");
    let missing = discover::catalogue(&project);
    assert!(missing.tasks.is_empty());
    assert!(missing.dependencies.contains(Path::new("common.mk")));
    for text in [
        "include Makefile\n",
        "include $(UNKNOWN)/Makefile\n",
        &"#".repeat(1024 * 1024 + 1),
    ] {
        write_source(&project, "common.mk", text);
        assert!(
            discover::tasks(&project).is_empty(),
            "an unresolved or cyclic owner must not restore a standalone child"
        );
    }
    write_source(&project, "common.mk", "include nested/Makefile\n");
    let resolved = discover::catalogue(&project);
    assert_eq!(
        resolved
            .tasks
            .iter()
            .map(|t| t.identity.name.as_str())
            .collect::<Vec<_>>(),
        ["root"]
    );
    assert!(resolved.dependencies.contains(Path::new("nested/Makefile")));
}

#[test]
fn make_import_chains_use_the_owners_cwd_and_nonconventional_grammar() {
    let project = Scratch::new("guild-import-chain-");
    write_source(
        &project,
        "Makefile",
        "include sub/common.mk\nroot:\n\t@echo root\n",
    );
    write_source(&project, "sub/common.mk", "include nested/Makefile\n");
    write_source(&project, "nested/Makefile", "child:\n\t@echo child\n");
    write_source(
        &project,
        "sub/nested/Makefile",
        "independent:\n\t@echo independent\n",
    );
    let tasks = discover::tasks(&project);
    assert!(tasks.iter().any(|t| t.identity.name == "root"));
    assert!(tasks.iter().any(|t| t.identity.name == "independent"));
    assert!(
        !tasks.iter().any(|t| t.identity.name == "child"),
        "a chained include is never a standalone invocation"
    );
}

#[test]
fn an_imported_makefile_is_not_analyzed_again_with_a_standalone_cwd() {
    let project = Scratch::new("guild-import-owner-cwd-");
    write_source(
        &project,
        "Makefile",
        "include common.mk\nroot:\n\t@echo root\n",
    );
    write_source(&project, "common.mk", "include nested/Makefile\n");
    write_source(
        &project,
        "nested/Makefile",
        "include shared.mk\nchild:\n\t@echo child\n",
    );
    write_source(
        &project,
        "shared.mk",
        "# exists only in the original invocation cwd\n",
    );
    write_source(&project, "nested/shared.mk", "include Makefile\n");
    let tasks = discover::tasks(&project);
    assert_eq!(
        tasks
            .iter()
            .map(|t| t.identity.name.as_str())
            .collect::<Vec<_>>(),
        ["root"],
        "the imported child's bogus standalone cwd must not invalidate its valid owner"
    );
}

#[test]
fn a_mutual_conventional_import_cycle_cannot_certify_either_owner() {
    let project = Scratch::new("guild-mutual-import-cycle-");
    write_source(
        &project,
        "Makefile",
        "include nested/Makefile\nroot:\n\t@echo root\n",
    );
    write_source(
        &project,
        "nested/Makefile",
        "include Makefile\nchild:\n\t@echo child\n",
    );
    assert!(
        discover::tasks(&project).is_empty(),
        "neither unresolved owner can certify the other's invocation context"
    );
}

#[test]
fn just_and_task_import_chains_keep_the_owners_grammar() {
    for (owner, common, child, owner_text, common_text, child_text) in [
        (
            "justfile",
            "shared/recipes.just",
            "nested/justfile",
            "import 'shared/recipes.just'\nroot:\n    echo root\n",
            "import '../nested/justfile'\n",
            "child:\n    echo child\n",
        ),
        (
            "Taskfile.yml",
            "shared/recipes.yaml",
            "nested/Taskfile.yml",
            "version: '3'\nincludes:\n  common: shared/recipes.yaml\ntasks:\n  root:\n    cmds: [echo root]\n",
            "version: '3'\nincludes:\n  child: ../nested/Taskfile.yml\n",
            "version: '3'\ntasks:\n  child:\n    cmds: [echo child]\n",
        ),
    ] {
        let project = Scratch::new("guild-import-chain-");
        write_source(&project, owner, owner_text);
        write_source(&project, common, common_text);
        write_source(&project, child, child_text);
        let tasks = discover::tasks(&project);
        assert!(tasks.iter().any(|t| t.identity.name == "root"), "{owner}");
        assert!(
            !tasks.iter().any(|t| t.identity.name == "child"),
            "{owner}: chained declarations inherit context"
        );
    }
}

#[test]
#[ignore = "explicit isolated native fixture evidence"]
fn native_fixture_source_discovery() {
    let project = std::env::var_os("GUILD_NATIVE_TASK_FIXTURE").expect("isolated fixture path");
    let tasks = discover::tasks(Path::new(&project));
    println!("{tasks:#?}");
    for runner in [Runner::Just, Runner::Make, Runner::Package, Runner::Task] {
        assert!(tasks.iter().any(|task| task.identity.runner == runner));
    }
}

#[test]
fn declarations_have_source_locations_and_required_just_parameters() {
    let tasks = source::parse(
        Path::new("nested/.Justfile"),
        "# example\nrun who suffix='x' +rest:\n    echo {{who}}\n",
    );
    assert_eq!(tasks.len(), 1);
    let task = &tasks[0];
    assert_eq!(task.row, 1);
    assert_eq!(task.identity.runner, Runner::Just);
    assert_eq!(task.identity.source, Path::new("nested/.Justfile"));
    assert_eq!(
        task.parameters
            .iter()
            .map(|p| (p.name.as_str(), p.required, p.variadic))
            .collect::<Vec<_>>(),
        [
            ("who", true, false),
            ("suffix", false, false),
            ("rest", true, true)
        ]
    );
    assert!(task.argv(&[], None).is_err());
}

#[test]
fn generated_shell_tokens_do_not_execute_metacharacters() {
    let tokens = vec![
        "printf".into(),
        "%s".into(),
        "a ' quote; $(touch NEVER)\nnext".into(),
    ];
    let output = std::process::Command::new("/bin/sh")
        .args(["-c", &source::shell_line(&tokens)])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), tokens[2]);
}

#[test]
fn argument_fields_preserve_quoted_empty_and_unicode_values() {
    assert_eq!(
        source::tokens("WHO='А Б' EMPTY=\"\" 'a;$(no)' ").unwrap(),
        ["WHO=А Б", "EMPTY=", "a;$(no)"]
    );
    assert!(source::tokens("'unfinished").is_err());
}

#[test]
fn unsupported_make_targets_and_malformed_documents_have_no_buttons() {
    let tasks = source::parse(
        Path::new("GNUmakefile"),
        "one two: prereq\n\t@echo ok\n%.o: %.c\n\tcc $<\n.PHONY: one\n$(computed):\n\techo no\n",
    );
    assert_eq!(
        tasks
            .iter()
            .map(|t| t.identity.name.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );
    assert!(source::parse(Path::new("package.json"), "{\"scripts\":{").is_empty());
    assert!(source::parse(Path::new("Taskfile.yml"), "version: '3'\ntasks: [broken").is_empty());
}

#[test]
fn imported_conventional_task_file_is_not_a_standalone_invocation() {
    let dir = Scratch::new("guild-task-import-");
    std::fs::create_dir(dir.join("module")).unwrap();
    std::fs::write(
        dir.join("justfile"),
        "import 'module/justfile'\nlocal:\n    echo ok\n",
    )
    .unwrap();
    std::fs::write(dir.join("module/justfile"), "imported:\n    echo no\n").unwrap();
    let tasks = discover::tasks(&dir);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].identity.name, "local");
}

#[test]
fn closest_manager_evidence_can_require_a_choice_and_explicit_wins() {
    let dir = Scratch::new("guild-task-manager-");
    std::fs::create_dir(dir.join("nested")).unwrap();
    std::fs::write(dir.join("yarn.lock"), "").unwrap();
    let file = Path::new("nested/package.json");
    assert_eq!(source::managers(&dir, file, &[]), ["yarn"]);
    std::fs::write(dir.join("nested/bun.lock"), "").unwrap();
    std::fs::write(dir.join("nested/pnpm-lock.yaml"), "").unwrap();
    assert_eq!(source::managers(&dir, file, &[]).len(), 2);
    assert_eq!(source::managers(&dir, file, &["npm".into()]), ["npm"]);
}

#[test]
fn task_object_includes_do_not_offer_the_imported_file() {
    let dir = Scratch::new("guild-task-includes-");
    std::fs::create_dir(dir.join("child")).unwrap();
    std::fs::write(dir.join("Taskfile.yml"), "version: '3'\nincludes:\n  child:\n    taskfile: ./child/Taskfile.yml\ntasks:\n  local:\n    cmds: [echo local]\n").unwrap();
    std::fs::write(
        dir.join("child/Taskfile.yml"),
        "version: '3'\ntasks:\n  imported:\n    cmds: [echo child]\n",
    )
    .unwrap();
    assert_eq!(
        discover::tasks(&dir)
            .iter()
            .map(|t| t.identity.name.as_str())
            .collect::<Vec<_>>(),
        ["local"]
    );
}

#[test]
fn implicit_modules_and_make_includes_keep_their_owner_context() {
    let dir = Scratch::new("guild-task-context-");
    std::fs::create_dir(dir.join("child")).unwrap();
    std::fs::write(dir.join("justfile"), "mod child\nlocal:\n    echo ok\n").unwrap();
    std::fs::write(dir.join("child/justfile"), "imported:\n    echo no\n").unwrap();
    std::fs::write(
        dir.join("Makefile"),
        "include child/Makefile\nlocal:\n\techo ok\n",
    )
    .unwrap();
    std::fs::write(dir.join("child/Makefile"), "imported:\n\techo no\n").unwrap();
    assert!(
        discover::tasks(&dir)
            .iter()
            .all(|task| task.identity.name == "local")
    );
}

#[test]
fn unknown_computed_import_context_is_not_silently_standalone() {
    let dir = Scratch::new("guild-task-unknown-");
    std::fs::create_dir(dir.join("child")).unwrap();
    std::fs::write(
        dir.join("Makefile"),
        "include $(MODULE)/Makefile\nlocal:\n\techo ok\n",
    )
    .unwrap();
    std::fs::write(dir.join("child/Makefile"), "imported:\n\techo no\n").unwrap();
    assert!(discover::tasks(&dir).is_empty());
}

#[test]
fn conditional_make_imports_cannot_expose_a_standalone_child() {
    let dir = Scratch::new("guild-task-conditional-import-");
    std::fs::create_dir(dir.join("child")).unwrap();
    std::fs::write(
        dir.join("Makefile"),
        "ifdef MODULE\ninclude child/Makefile\nendif\nlocal:\n\techo ok\n",
    )
    .unwrap();
    std::fs::write(dir.join("child/Makefile"), "imported:\n\techo no\n").unwrap();
    assert!(
        discover::tasks(&dir)
            .iter()
            .all(|task| task.identity.name == "local")
    );
    std::fs::write(
        dir.join("Makefile"),
        "ifdef MODULE\ninclude $(MODULE)/Makefile\nendif\nlocal:\n\techo ok\n",
    )
    .unwrap();
    assert!(discover::tasks(&dir).is_empty());
}

#[test]
fn an_explicit_project_root_is_not_hidden_by_its_parent_checkout() {
    let outer = Scratch::new("guild-task-parent-ignore-");
    std::fs::write(outer.join(".gitignore"), ".guild/\n").unwrap();
    let project = outer.join(".guild/fixture");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    std::fs::write(project.join("justfile"), "check:\n    echo ok\n").unwrap();
    assert_eq!(discover::tasks(&project).len(), 1);
}

#[test]
fn discovery_tracks_ignored_sources_case_renames_deletion_and_manager_changes() {
    let dir = Scratch::new("guild-task-refresh-");
    std::fs::create_dir(dir.join("ignored")).unwrap();
    std::fs::write(dir.join(".gitignore"), "ignored/\n").unwrap();
    std::fs::write(dir.join("ignored/Justfile"), "no:\n    echo no\n").unwrap();
    std::fs::write(dir.join("Justfile"), "before:\n    echo ok\n").unwrap();
    assert_eq!(discover::tasks(&dir).len(), 1);
    std::fs::rename(dir.join("Justfile"), dir.join("GNUmakefile")).unwrap();
    std::fs::write(dir.join("GNUmakefile"), "after:\n\techo ok\n").unwrap();
    assert_eq!(discover::tasks(&dir)[0].identity.runner, Runner::Make);
    std::fs::remove_file(dir.join("GNUmakefile")).unwrap();
    assert!(discover::tasks(&dir).is_empty());
    std::fs::write(
        dir.join("package.json"),
        r#"{"scripts":{"check":"echo ok"}}"#,
    )
    .unwrap();
    assert_eq!(discover::tasks(&dir)[0].managers, ["npm"]);
    std::fs::write(dir.join("bun.lock"), "").unwrap();
    assert_eq!(discover::tasks(&dir)[0].managers, ["bun"]);
    std::fs::remove_file(dir.join("bun.lock")).unwrap();
    std::fs::write(dir.join("pnpm-lock.yaml"), "").unwrap();
    assert_eq!(discover::tasks(&dir)[0].managers, ["pnpm"]);
}
