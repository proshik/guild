//! A command a person runs: the argv it becomes, and how long it took.
//!
//! Everything here is a pure function — no pty, no window, no project.
//! What the pty does with the argv was measured in task 0 of M8.4 and is
//! checked where the emulator is.

use std::time::Duration;

use crate::scratch::Scratch;
use guild_desktop::run;

/// The command has to run in the shell the person configured, for the
/// reason a terminal tab does: their aliases, their functions, their
/// `PATH`. `-l` is not decoration — on macOS the `PATH` is assembled at
/// login, and a bundled `Guild.app` inherits one from `launchd` without
/// `/opt/homebrew/bin` in it, which is where `just` lives.
#[test]
fn a_command_is_run_by_the_shell_the_person_chose() {
    assert_eq!(
        run::argv("just check", Some("/opt/homebrew/bin/fish")),
        vec!["/opt/homebrew/bin/fish", "-l", "-c", "just check"]
    );
}

/// `SHELL` may be unset — that happens in a stripped-down environment. A
/// run that never started is worse than one in an unfamiliar shell.
#[test]
fn without_a_setting_a_shell_that_exists_everywhere_runs_it() {
    assert_eq!(run::argv("just check", None)[0], "/bin/sh");
}

/// An empty variable means "unset" rather than "a shell with an empty
/// name": the second would fail to spawn at all.
#[test]
fn an_empty_shell_setting_counts_as_absent() {
    assert_eq!(run::argv("just check", Some("   "))[0], "/bin/sh");
}

/// The line reaches the shell whole, quotes, pipes and all: a person
/// writes what they would have typed.
#[test]
fn the_line_reaches_the_shell_as_it_was_written() {
    let argv = run::argv("cargo test -- --nocapture | head -20", Some("/bin/zsh"));

    assert_eq!(argv[3], "cargo test -- --nocapture | head -20");
}

/// Nothing is not a command. The Run button offers the list instead of
/// starting a shell that exits at once and reports success.
#[test]
fn a_blank_line_is_not_a_command() {
    assert!(!run::runnable(""));
    assert!(!run::runnable("   \t "));
    assert!(run::runnable("just check"));
}

/// How long it took, the way a person reads a clock rather than a
/// duration: seconds alone under a minute, then minutes with the seconds
/// padded so the number does not jump about as it counts.
#[test]
fn the_time_taken_is_printed_as_a_person_reads_it() {
    assert_eq!(run::elapsed(Duration::from_millis(400)), "0s");
    assert_eq!(run::elapsed(Duration::from_secs(3)), "3s");
    assert_eq!(run::elapsed(Duration::from_secs(59)), "59s");
    assert_eq!(run::elapsed(Duration::from_secs(60)), "1m 00s");
    assert_eq!(run::elapsed(Duration::from_secs(64)), "1m 04s");
    assert_eq!(run::elapsed(Duration::from_secs(750)), "12m 30s");
    assert_eq!(run::elapsed(Duration::from_secs(3600)), "1h 00m");
    assert_eq!(run::elapsed(Duration::from_secs(3600 + 240)), "1h 04m");
}

/// A run that ended is one of three things, and only one of them is a
/// number: the code, a stop we asked for, or a command that never
/// started. **Stopped is ours to know** — task 0 found that a killed
/// process reports exit code 1, exactly as a command that genuinely
/// failed with 1 does, so it can never be read back out of the status.
#[test]
fn a_finished_run_says_which_of_the_three_it_is() {
    assert!(!run::State::Running.is_over());
    assert!(run::State::Finished { code: 0 }.went_well());
    assert!(!run::State::Finished { code: 3 }.went_well());
    assert!(!run::State::Stopped.went_well());
    assert!(
        run::State::Failed {
            reason: "no such file".into()
        }
        .is_over()
    );
}

/// What `just --summary` prints is one line of names, and each becomes a
/// command a person can run. The order is `just`'s own — it is
/// alphabetical, and re-sorting it would only be a second opinion.
#[test]
fn a_justfile_becomes_one_command_per_recipe() {
    let recipes = run::discover::just_recipes("app boundaries build check\n");

    assert_eq!(
        recipes,
        vec![
            "just app".to_owned(),
            "just boundaries".to_owned(),
            "just build".to_owned(),
            "just check".to_owned()
        ]
    );
}

/// A repository with a `justfile` that declares nothing, and stray
/// whitespace around what it does declare: neither is an error, and
/// neither leaves an empty row in the list.
#[test]
fn an_empty_summary_declares_nothing() {
    assert!(run::discover::just_recipes("").is_empty());
    assert!(run::discover::just_recipes("   \n  \n").is_empty());
    assert_eq!(
        run::discover::just_recipes("  check   test \n"),
        vec!["just check".to_owned(), "just test".to_owned()]
    );
}

/// The second row of the table, so that it is a table: a `package.json`
/// declares its scripts, and each is `npm run <name>`.
///
/// **Sorted, deliberately.** The first version of this test expected the
/// file's own order and failed: `serde_json`'s map preserves insertion
/// order when something in the graph enables `preserve_order`, and
/// something in ours does, though nothing of ours asked for it. An order
/// that depends on a feature flag three crates away is not an order.
#[test]
fn a_package_json_becomes_one_command_per_script() {
    let package = r#"{ "name": "x", "scripts": { "test": "vitest", "build": "vite build" } }"#;

    assert_eq!(
        run::discover::npm_scripts(package),
        vec!["npm run build".to_owned(), "npm run test".to_owned()]
    );
}

/// A file that declares no scripts, one whose `scripts` is not an object,
/// and one that is not JSON at all: all three are "nothing to offer"
/// rather than a failure. A project is not obliged to be tidy for us.
#[test]
fn a_package_json_without_scripts_declares_nothing() {
    assert!(run::discover::npm_scripts(r#"{ "name": "x" }"#).is_empty());
    assert!(run::discover::npm_scripts(r#"{ "scripts": "all of them" }"#).is_empty());
    assert!(run::discover::npm_scripts("not json at all {").is_empty());
}

/// **One row is not a table.** M8.1.1 paid for a second capture list that
/// nobody could see drifting; the shape here is the same bet, and it is
/// only a bet if there is more than one row in it.
#[test]
fn a_runner_is_a_row_and_there_is_more_than_one() {
    let files: Vec<&str> = run::discover::RUNNERS.iter().map(|r| r.file).collect();

    assert!(files.len() >= 2, "one row is not a table");
    assert!(files.contains(&"justfile"));
    assert!(files.contains(&"package.json"));
    let mut sorted = files.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        files.len(),
        "two rows must not claim one file"
    );
}

/// A project that declares nothing at all — no `justfile`, no
/// `package.json` — offers an empty list rather than a list of guesses.
/// Decision 8: the window never invents a command.
#[test]
fn a_project_that_declares_nothing_offers_nothing() {
    let dir = Scratch::new("guild-run-bare-");

    assert!(run::discover::declared(&dir).is_empty());
}

#[test]
fn discovery_reads_nested_make_targets_without_evaluating_project_code() {
    let dir = Scratch::new("guild-run-source-");
    std::fs::create_dir(dir.join("nested")).unwrap();
    std::fs::write(
        dir.join("nested/Makefile"),
        "build test:\n\t@echo ok\n%.o: %.c\n\tcc $<\n.PHONY: build test\n",
    )
    .unwrap();
    let commands = run::discover::declared(&dir);
    assert_eq!(
        commands.len(),
        2,
        "literal nested Make targets must be offered"
    );
    assert!(commands.iter().any(|line| line.contains("build")));
    assert!(commands.iter().any(|line| line.contains("test")));
}

#[test]
fn discovery_reads_just_source_and_never_runs_backticks() {
    let dir = Scratch::new("guild-run-no-evaluation-");
    std::fs::write(
        dir.join("Justfile"),
        "value := `touch evaluated`\ncheck:\n    echo ok\n",
    )
    .unwrap();
    let commands = run::discover::declared(&dir);
    assert_eq!(commands.len(), 1);
    assert!(commands[0].contains("check"));
    assert!(!dir.join("evaluated").exists());
}

#[test]
fn discovery_offers_taskfile_and_honors_package_manager() {
    let dir = Scratch::new("guild-run-four-formats-");
    std::fs::write(dir.join("Taskfile.yaml"), "version: '3'\ntasks:\n  default:\n    cmds: [echo ok]\n  test:\n    cmds:\n      - echo test\n").unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{"packageManager":"pnpm@10.0.0","scripts":{"build":"echo ok","bad":42}}"#,
    )
    .unwrap();
    let commands = run::discover::declared(&dir);
    assert_eq!(commands.len(), 3);
    assert!(commands.iter().any(|line| line.contains("pnpm")));
}

/// The state line is the reason the panel exists, so it says the code in
/// both languages and never hides it behind a word like "failed".
#[test]
fn the_state_line_says_the_code() {
    use guild_desktop::i18n::{Language, Ui};

    let english = Ui(Language::English);
    let russian = Ui(Language::Russian);

    assert_eq!(
        english.run_state(&run::State::Finished { code: 0 }, "1m 04s"),
        "exit 0 · 1m 04s"
    );
    assert!(
        russian
            .run_state(&run::State::Finished { code: 101 }, "8s")
            .contains("101"),
        "the code survives translation"
    );
    assert!(english.run_state(&run::State::Running, "3s").contains("3s"));
    assert_ne!(
        english.run_state(&run::State::Stopped, "3s"),
        english.run_state(&run::State::Finished { code: 1 }, "3s"),
        "a run we stopped must not read as one that failed with 1 — they \
         are the same exit code, which is exactly why the window keeps the \
         difference itself"
    );
}

/// The list a person chooses from: what they are typing, what they have
/// run here, and what the project declares — in that order, because that
/// is the order they are likely to reach for.
#[test]
fn the_list_offers_what_was_typed_then_the_recent_then_the_declared() {
    let recent = ["just check".to_owned()];
    let declared = ["just check".to_owned(), "just test".to_owned()];

    let rows = run::rows("", &recent, &declared);
    let lines: Vec<&str> = rows.iter().map(|row| row.line.as_str()).collect();

    assert_eq!(
        lines,
        ["just check", "just test"],
        "a command both run and declared is one row, and it is the recent one"
    );
    assert_eq!(rows[0].source, run::Source::Recent);
    assert_eq!(rows[1].source, run::Source::Declared);
}

/// Typing narrows the list and, whenever what is typed is not already a
/// row, offers to run it — the branch popup's create row in another hat.
/// **Not only when nothing matched:** `just c` matches `just check`, and
/// the offer to run `just c` is still right, because a command is a line
/// and not a name.
#[test]
fn typing_offers_to_run_what_was_typed() {
    let recent = ["just check".to_owned()];
    let declared = ["just test".to_owned()];

    let rows = run::rows("just c", &recent, &declared);
    assert_eq!(rows[0].source, run::Source::Typed);
    assert_eq!(rows[0].line, "just c");
    assert_eq!(rows[1].line, "just check", "and the match is still offered");

    let exact = run::rows("just check", &recent, &declared);
    assert_eq!(
        exact[0].source,
        run::Source::Recent,
        "a line that is already a row is not offered twice"
    );

    let nothing = run::rows("cargo doc --open", &recent, &declared);
    assert_eq!(nothing.len(), 1, "nothing matched, so only the offer");
    assert_eq!(nothing[0].source, run::Source::Typed);
}

/// A recipe is found by its scattered letters, as every popup's list is
/// since M8.9, and **what was typed stays first**: it is the only row that
/// runs exactly what the person wrote.
#[test]
fn a_recipe_is_found_by_its_scattered_letters_and_what_was_typed_stays_first() {
    let recent = ["just check".to_owned()];
    let declared = ["just test".to_owned(), "just clippy".to_owned()];

    let rows = run::rows("jck", &recent, &declared);
    let lines: Vec<&str> = rows.iter().map(|row| row.line.as_str()).collect();
    assert_eq!(lines, ["jck", "just check"]);
    assert_eq!(rows[0].source, run::Source::Typed);

    let rows = run::rows("jclp", &recent, &declared);
    let lines: Vec<&str> = rows.iter().map(|row| row.line.as_str()).collect();
    assert_eq!(lines, ["jclp", "just clippy"]);
}

/// A project that declares nothing and has run nothing offers nothing —
/// and an empty query offers no row to run, because there is nothing to
/// run. Decision 8: the window never invents a command.
#[test]
fn an_empty_list_is_empty() {
    assert!(run::rows("", &[], &[]).is_empty());
    assert!(run::rows("   ", &[], &[]).is_empty());
}
