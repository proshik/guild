//! What a project depends on, read from what its build system answered —
//! M8.14. The Gradle, Cargo and Go answers here are cut from real ones
//! captured in the plan's task 0; npm, Bundler and Mix are read from
//! files, so theirs are directories laid out as those tools leave them.

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_desktop::deps::{self, Answer, Source, Stack};

fn temp(name: &str) -> Scratch {
    Scratch::new(&format!("guild-deps-{name}-"))
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn labels(answer: &Answer) -> Vec<&str> {
    answer
        .dependencies
        .iter()
        .map(|d| d.label.as_str())
        .collect()
}

/// **The labels are the familiar ones**, `Gradle: group:module:version`,
/// sorted as an External Libraries node sorts them; a dependency every
/// subproject has is one row, and whatever else Gradle prints is not read
/// as one.
#[test]
fn gradle_lists_every_dependency_once() {
    let output = "\
GUILD-DEP\tch.qos.logback:logback-core:1.5.21\t/g/files-2.1/ch.qos.logback/logback-core/1.5.21/aa/logback-core-1.5.21.jar
GUILD-DEP\tandroidx.annotation:annotation:1.0.0\t/g/files-2.1/androidx.annotation/annotation/1.0.0/bb/annotation-1.0.0.jar
Configuration cache is an incubating feature.
GUILD-DEP\tch.qos.logback:logback-core:1.5.21\t/g/files-2.1/ch.qos.logback/logback-core/1.5.21/aa/logback-core-1.5.21.jar
GUILD-JDK\t21\t/opt/homebrew/Cellar/openjdk@21/21.0.11/libexec/openjdk.jdk/Contents/Home
GUILD-JDK\t21\t/opt/homebrew/Cellar/openjdk@21/21.0.11/libexec/openjdk.jdk/Contents/Home
";
    let answer = deps::gradle(output);
    assert_eq!(
        labels(&answer),
        [
            "Gradle: androidx.annotation:annotation:1.0.0",
            "Gradle: ch.qos.logback:logback-core:1.5.21",
        ],
        "one row each, in order"
    );
    assert_eq!(
        answer.dependencies[1].source,
        Source::Jar(PathBuf::from(
            "/g/files-2.1/ch.qos.logback/logback-core/1.5.21/aa/logback-core-1.5.21.jar"
        ))
    );
    assert_eq!(answer.jdks.len(), 1, "the JDK once");
    assert_eq!(answer.jdks[0].label, "< 21 >");
    assert!(
        answer.jdks[0]
            .sources()
            .ends_with("Contents/Home/lib/src.zip")
    );
}

/// Maven's `dependency:list` with absolute file names: a classifier is a
/// field more, and a modular jar says ` -- module name` after its path.
#[test]
fn maven_reads_its_list_with_or_without_a_classifier() {
    let output = "\
[INFO] The following files have been resolved:
   commons-io:commons-io:jar:2.16.1:compile:/Users/person/.m2/repository/commons-io/commons-io/2.16.1/commons-io-2.16.1.jar -- module org.apache.commons.io
   org.lwjgl:lwjgl:jar:natives-macos:3.3.3:runtime:/Users/person/.m2/repository/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-macos.jar
";
    let answer = deps::maven(output);
    assert_eq!(
        labels(&answer),
        [
            "Maven: commons-io:commons-io:2.16.1",
            "Maven: org.lwjgl:lwjgl:3.3.3"
        ]
    );
    assert_eq!(
        answer.dependencies[0].source,
        Source::Jar(PathBuf::from(
            "/Users/person/.m2/repository/commons-io/commons-io/2.16.1/commons-io-2.16.1.jar"
        ))
    );
}

/// Cargo: every package outside the workspace, read where its manifest is.
#[test]
fn cargo_lists_what_is_not_the_workspace() {
    let json = r#"{
      "packages": [
        {"id": "path+file:///w/crates/app#0.1.0", "name": "app", "version": "0.1.0",
         "manifest_path": "/w/crates/app/Cargo.toml"},
        {"id": "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.228",
         "name": "serde", "version": "1.0.228",
         "manifest_path": "/Users/person/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde-1.0.228/Cargo.toml"}
      ],
      "workspace_members": ["path+file:///w/crates/app#0.1.0"]
    }"#;
    let answer = deps::cargo(json);
    assert_eq!(labels(&answer), ["serde 1.0.228"]);
    assert_eq!(
        answer.dependencies[0].source,
        Source::Dir(PathBuf::from(
            "/Users/person/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde-1.0.228"
        ))
    );
    assert_eq!(deps::cargo("not json"), Answer::default());
}

/// Go: the main module is not a dependency, a module not yet downloaded is
/// listed as missing, and a stream of objects is not one JSON document.
#[test]
fn go_reads_a_stream_and_leaves_out_the_main_module() {
    let json = r#"{"Path": "kotlyar_loyalty_machine", "Main": true, "Dir": "/p/kotlyar_loyalty_machine", "GoVersion": "1.26"}
{"Path": "dario.cat/mergo", "Version": "v1.0.2", "Indirect": true, "Dir": "/Users/person/go/pkg/mod/dario.cat/mergo@v1.0.2", "GoVersion": "1.13"}
{"Path": "filippo.io/edwards25519", "Version": "v1.2.0", "Indirect": true}
"#;
    let answer = deps::go(json);
    assert_eq!(
        labels(&answer),
        ["dario.cat/mergo@v1.0.2", "filippo.io/edwards25519@v1.2.0"]
    );
    assert_eq!(
        answer.dependencies[0].source,
        Source::Dir(PathBuf::from(
            "/Users/person/go/pkg/mod/dario.cat/mergo@v1.0.2"
        ))
    );
    assert_eq!(answer.dependencies[1].source, Source::Missing);
}

/// npm, pnpm and yarn leave the same thing behind: `node_modules`, a
/// scope one level deeper, and pnpm's store behind a dot.
#[test]
fn npm_reads_node_modules_and_its_scopes() {
    let root = temp("npm");
    write(&root.join("package.json"), "{}");
    let modules = root.join("node_modules");
    write(
        &modules.join("prettier/package.json"),
        r#"{"name":"prettier","version":"3.3.3"}"#,
    );
    write(
        &modules.join("@types/node/package.json"),
        r#"{"name":"@types/node","version":"22.5.0"}"#,
    );
    write(&modules.join(".pnpm/lock.yaml"), "");
    write(&modules.join("broken/package.json"), "not json");

    let answer = deps::npm(&root);
    assert_eq!(labels(&answer), ["@types/node@22.5.0", "prettier@3.3.3"]);
    assert_eq!(
        answer.dependencies[1].source,
        Source::Dir(modules.join("prettier"))
    );
    assert_eq!(
        deps::npm(&temp("npm-none")),
        Answer::default(),
        "nothing installed"
    );
}

/// Bundler, by the lockfile: a spec is indented four and what it needs
/// six; a gem is found in the project's `vendor/bundle` first and then in
/// the Rubies' own gem directories; one no Ruby has is listed as missing.
#[test]
fn bundler_finds_what_the_lockfile_names() {
    let root = temp("bundler");
    write(
        &root.join("Gemfile.lock"),
        "\
GEM
  remote: https://rubygems.org/
  specs:
    rake (13.2.1)
    rspec (3.13.0)
      rspec-core (~> 3.13.0)
    rspec-core (3.13.1)

PLATFORMS
  arm64-darwin-24

BUNDLED WITH
   4.0.11
",
    );
    let vendored = root.join("vendor/bundle/ruby/4.0.0/gems/rake-13.2.1");
    std::fs::create_dir_all(&vendored).unwrap();
    let homebrew = temp("bundler-gems");
    std::fs::create_dir_all(homebrew.join("gems/rspec-3.13.0")).unwrap();

    let answer = deps::bundler(&root, &[homebrew.to_path_buf()]);
    assert_eq!(
        labels(&answer),
        ["rake (13.2.1)", "rspec (3.13.0)", "rspec-core (3.13.1)"],
        "a requirement of a spec is not a spec"
    );
    assert_eq!(answer.dependencies[0].source, Source::Dir(vendored));
    assert_eq!(
        answer.dependencies[1].source,
        Source::Dir(homebrew.join("gems/rspec-3.13.0"))
    );
    assert_eq!(answer.dependencies[2].source, Source::Missing);
}

/// Mix, by the lockfile: a Hex package's version, a git dependency's
/// commit, and the sources in the project's `deps/`.
#[test]
fn mix_reads_hex_and_git_entries() {
    let root = temp("mix");
    write(&root.join("mix.exs"), "");
    write(
        &root.join("mix.lock"),
        r#"%{
  "jason": {:hex, :jason, "1.4.4", "b9226785a9aa77b6857ca22832cffa5d5011a667207eb2a0ad56adb5db443b8a", [:mix], [], "hexpm", "c5eb0cab91f094599f94d55bc63409236a8ec69a21a67814529e8d5f6cc90b3b"},
  "thing": {:git, "https://github.com/p/thing.git", "0123abcd4567ef890123abcd4567ef8901234567", [branch: "main"]},
}
"#,
    );
    std::fs::create_dir_all(root.join("deps/jason")).unwrap();

    let answer = deps::mix(&root);
    assert_eq!(labels(&answer), ["jason 1.4.4", "thing 0123abc"]);
    assert_eq!(
        answer.dependencies[0].source,
        Source::Dir(root.join("deps/jason"))
    );
    assert_eq!(
        answer.dependencies[1].source,
        Source::Missing,
        "not fetched"
    );
}

/// A project says which stacks it has by the files at its root.
#[test]
fn a_project_s_stacks_are_its_manifests() {
    let root = temp("stacks");
    write(&root.join("settings.gradle.kts"), "");
    write(&root.join("package.json"), "{}");
    assert_eq!(Stack::of(&root), [Stack::Gradle, Stack::Npm]);
    assert!(Stack::of(&temp("stacks-none")).is_empty());
}

/// Two stacks in one project are one list, each row once.
#[test]
fn two_answers_merge_in_order() {
    let npm = deps::go(r#"{"Path": "b.io/x", "Version": "v1", "Dir": "/x"}"#);
    let gradle = deps::gradle("GUILD-DEP\ta:a:1\t/a.jar\nGUILD-JDK\t21\t/jdk\n");
    let merged = npm.merge(gradle.clone()).merge(gradle);
    assert_eq!(labels(&merged), ["b.io/x@v1", "Gradle: a:a:1"]);
    assert_eq!(merged.jdks.len(), 1);
}

/// **A build system that does not answer is killed at the deadline**,
/// and one that refuses says why — the end of what it printed, which is
/// where a build tool puts its reason.
#[test]
fn asking_has_a_deadline_and_a_reason() {
    use std::time::{Duration, Instant};
    let dir = temp("ask-run");

    let started = Instant::now();
    let slow = deps::ask::run(&dir, Path::new("sleep"), &["5"], Duration::from_millis(300));
    assert!(
        slow.as_ref()
            .is_err_and(|why| why.contains("did not answer")),
        "{slow:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "killed, not waited for"
    );

    let refused = deps::ask::run(
        &dir,
        Path::new("sh"),
        &["-c", "echo first >&2; echo 'no such task' >&2; exit 3"],
        Duration::from_secs(5),
    );
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.contains("no such task")),
        "{refused:?}"
    );

    let answered = deps::ask::run(
        &dir,
        Path::new("sh"),
        &["-c", "echo ok"],
        Duration::from_secs(5),
    );
    assert_eq!(answered.as_deref(), Ok("ok\n"));
}

/// Cargo and Go asked for real, on projects with nothing to download: the
/// answer is empty and it *is* an answer. Skipped, and said so, where the
/// tool is not installed.
#[test]
fn cargo_and_go_are_asked_for_real() {
    let scratch = temp("ask-scratch");
    let has = |tool: &str| {
        std::process::Command::new(tool)
            .arg("version")
            .output()
            .is_ok()
    };

    if has("cargo") {
        let root = temp("ask-cargo");
        write(
            &root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        write(&root.join("src/lib.rs"), "");
        let answer = deps::ask::ask(&root, Stack::Cargo, &scratch).expect("cargo answered");
        assert!(answer.dependencies.is_empty(), "{answer:?}");
    } else {
        eprintln!("no cargo on this machine; nothing claimed");
    }

    if has("go") {
        let root = temp("ask-go");
        write(
            &root.join("go.mod"),
            "module example.com/fixture\n\ngo 1.21\n",
        );
        let answer = deps::ask::ask(&root, Stack::Go, &scratch).expect("go answered");
        assert!(
            answer.dependencies.is_empty(),
            "the main module is not one: {answer:?}"
        );
    } else {
        eprintln!("no go on this machine; nothing claimed");
    }
}

/// **What a stack answered is kept, and read back by the same reader**,
/// so a reopened project lists at once while it is asked again — task 4.
/// The tool's own answer is what is kept: there is no second format to
/// drift from the reader. A stack read from files is read again.
#[test]
fn a_kept_answer_is_read_back_by_the_same_reader() {
    let scratch = temp("kept");
    let root = temp("kept-root");
    assert_eq!(
        deps::ask::cached(&root, Stack::Gradle, &scratch),
        None,
        "nothing kept yet"
    );

    let gradle = "GUILD-DEP\ta:a:1\t/a.jar\nGUILD-JDK\t21\t/jdk\n";
    write(&scratch.join("gradle.answer"), gradle);
    assert_eq!(
        deps::ask::cached(&root, Stack::Gradle, &scratch),
        Some(deps::gradle(gradle))
    );

    // A real asking keeps what it was told.
    let has_cargo = std::process::Command::new("cargo")
        .arg("version")
        .output()
        .is_ok();
    if has_cargo {
        write(
            &root.join("Cargo.toml"),
            "[package]\nname = \"kept\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        write(&root.join("src/lib.rs"), "");
        let asked = deps::ask::ask(&root, Stack::Cargo, &scratch).expect("cargo answered");
        assert_eq!(
            deps::ask::cached(&root, Stack::Cargo, &scratch),
            Some(asked)
        );
    }

    write(
        &root.join("node_modules/left-pad/package.json"),
        r#"{"name":"left-pad","version":"1.3.0"}"#,
    );
    assert_eq!(
        deps::ask::cached(&root, Stack::Npm, &scratch).map(|a| a.dependencies.len()),
        Some(1),
        "files are read again, not kept"
    );
}

/// **Maven colours its list even into a file** — measured on this
/// machine: `-DoutputFile` got `…commons-io-2.16.1.jar\x1b[36m -- module
/// org.apache.commons.io\x1b[m`, and a reader that split on ` -- ` took
/// the colour code as part of the jar's path. The line here is that one.
#[test]
fn maven_s_colours_are_not_part_of_a_path() {
    let output = "\nThe following files have been resolved:\n   commons-io:commons-io:jar:2.16.1:compile:/Users/person/.m2/repository/commons-io/commons-io/2.16.1/commons-io-2.16.1.jar\u{1b}[36m -- module org.apache.commons.io\u{1b}[m\n";
    let answer = deps::maven(output);
    assert_eq!(labels(&answer), ["Maven: commons-io:commons-io:2.16.1"]);
    assert_eq!(
        answer.dependencies[0].source,
        Source::Jar(PathBuf::from(
            "/Users/person/.m2/repository/commons-io/commons-io/2.16.1/commons-io-2.16.1.jar"
        ))
    );
}

#[test]
fn a_build_answer_does_not_wait_for_an_inheriting_descendant() {
    use std::time::Duration;
    let dir = temp("inherited-output");
    let root = dir.to_path_buf();
    let (send, receive) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let result = deps::ask::run(
            &root,
            Path::new("/bin/sh"),
            &["-c", "/bin/sleep 30 & echo $! > child.pid; echo answer"],
            Duration::from_secs(1),
        );
        let _ = send.send(result);
    });
    let result = receive.recv_timeout(Duration::from_secs(3));
    if result.is_err()
        && let Ok(pid) = std::fs::read_to_string(dir.join("child.pid"))
    {
        unsafe {
            libc::kill(pid.trim().parse().unwrap(), libc::SIGKILL);
        }
    }
    thread.join().unwrap();
    assert_eq!(
        result
            .expect("inherited pipes must not bypass the deadline")
            .as_deref(),
        Ok("answer\n")
    );
}

#[test]
fn diagnostic_encoding_does_not_replace_the_command_outcome() {
    use std::time::Duration;
    let dir = temp("diagnostic-encoding");
    let ask = |script: &str| {
        deps::ask::run(
            &dir,
            Path::new("/bin/sh"),
            &["-c", script],
            Duration::from_secs(10),
        )
    };
    assert_eq!(
        ask(r"printf 'answer\n'; printf '\377' >&2").as_deref(),
        Ok("answer\n")
    );
    let failure = ask(r"printf '\377 no such task\n' >&2; exit 7").unwrap_err();
    assert!(
        failure.contains("exited with") && failure.contains("no such task"),
        "{failure}"
    );
    let failure = ask(r"printf '\377'; printf 'bad build\n' >&2; exit 7").unwrap_err();
    assert!(
        failure.contains("exited with") && failure.contains("bad build"),
        "{failure}"
    );
    assert!(
        ask(r"printf '\377'").is_err(),
        "structured stdout must still be valid UTF-8"
    );
}
