//! Which language a file is, and whether it is text at all. Out here
//! because the file tab asks both through the public door.

use std::path::Path;

use crate::scratch::Scratch;
use guild_desktop::highlight::{Indent, Language, LineEnding, PLAIN_INDENT, Role, Text};

/// The name of the language a file is, or `"—"` when nothing claims it.
fn name_of(path: &str) -> &'static str {
    Language::of(Path::new(path)).map_or("—", |language| language.name)
}

/// The grammar that parses a file, which is not always the language's own
/// name — that is the whole point of the two being separate fields.
fn grammar_of(path: &str) -> &'static str {
    Language::of(Path::new(path)).map_or("—", |language| language.grammar)
}

/// A file's language comes from its name: the whole name first, then the
/// suffixes from the longest.
#[test]
fn a_language_is_known_by_the_file_name() {
    assert_eq!(name_of("src/lib.rs"), "rust");
    assert_eq!(name_of("Cargo.toml"), "toml");
    assert_eq!(name_of("justfile"), "just");
    assert_eq!(name_of(".zshrc"), "bash");
    assert_eq!(name_of("docs/plan.md"), "markdown");
    assert_eq!(name_of(".github/workflows/check.yml"), "yaml");
    assert_eq!(name_of("data.json"), "json");
    assert_eq!(name_of("Makefile"), "make");
    assert_eq!(name_of("api/user.proto"), "proto");
    assert_eq!(name_of("Dockerfile"), "dockerfile");
    assert_eq!(name_of("app/main.ex"), "elixir");
    assert_eq!(name_of("Main.kt"), "kotlin");
    assert_eq!(name_of("app.tsx"), "tsx");
    assert_eq!(name_of("LIB.RS"), "rust", "case does not matter");
    assert_eq!(name_of("notes.txt"), "—", "text is not a language");
    assert_eq!(
        name_of(".gitignore"),
        "—",
        "a leading dot makes a hidden file, not an extension"
    );
}

/// The fourteen of M8.8.6, by the name a person would open.
///
/// `.h` is C's rather than C++'s — a header names no language and every
/// editor has to choose — and `.svg` is XML, which is the only claim that
/// row makes about an SVG.
#[test]
fn the_fourteen_are_known_by_their_files() {
    assert_eq!(name_of("index.html"), "html");
    assert_eq!(name_of("site/main.css"), "css");
    assert_eq!(name_of("theme.scss"), "scss");
    assert_eq!(name_of("App.svelte"), "svelte");
    assert_eq!(name_of("pom.xml"), "xml");
    assert_eq!(name_of("logo.svg"), "xml");
    assert_eq!(name_of("schema.sql"), "sql");
    assert_eq!(name_of("tool.py"), "python");
    assert_eq!(name_of("parser.c"), "c");
    assert_eq!(name_of("parser.h"), "c");
    assert_eq!(name_of("engine.cpp"), "cpp");
    assert_eq!(name_of("engine.hpp"), "cpp");
    assert_eq!(name_of("Program.cs"), "csharp");
    assert_eq!(name_of("index.php"), "php");
    assert_eq!(name_of("App.swift"), "swift");
    assert_eq!(name_of("init.lua"), "lua");
    assert_eq!(name_of("flake.nix"), "nix");
    assert_eq!(
        Language::of_file(Path::new("tools/build"), "#!/usr/bin/env python3")
            .map_or("—", |l| l.name),
        "python"
    );
}

/// `lock` is not a language. One suffix, four parsers and one refusal —
/// which is what the grammar being its own field is for.
#[test]
fn a_lock_file_is_read_by_whatever_wrote_it() {
    assert_eq!(grammar_of("Cargo.lock"), "toml");
    assert_eq!(grammar_of("poetry.lock"), "toml");
    assert_eq!(grammar_of("package-lock.json"), "json");
    assert_eq!(grammar_of("flake.lock"), "json");
    assert_eq!(grammar_of("mix.lock"), "elixir");
    assert_eq!(
        grammar_of("Gemfile.lock"),
        "—",
        "its own format, and nobody asked"
    );
    assert_eq!(
        grammar_of("whatever.lock"),
        "—",
        "the suffix claims nothing"
    );
}

/// A language rides another's grammar where the shapes agree, and says
/// its own name while doing it.
#[test]
fn a_language_may_ride_another_grammar() {
    assert_eq!(name_of("app.ini"), "ini");
    assert_eq!(grammar_of("app.ini"), "toml");
    assert_eq!(name_of(".editorconfig"), "ini");
    assert_eq!(grammar_of(".editorconfig"), "toml");
    assert_eq!(name_of("tsconfig.jsonc"), "jsonc");
    assert_eq!(grammar_of("tsconfig.jsonc"), "json");
}

/// The whole name beats the extension, and the longest suffix beats the
/// shortest — the rule `icons::for_file` has had since M8.1.
#[test]
fn the_whole_name_beats_the_extension() {
    assert_eq!(grammar_of("mix.lock"), "elixir");
    assert_eq!(grammar_of("some/path/mix.lock"), "elixir");
    assert_eq!(name_of("docker-compose.yml"), "yaml");
}

/// The first line is read only when the name said nothing at all. A
/// shebang does not change while the three hundredth line is typed,
/// which is what M8.1 refused content for.
#[test]
fn a_shebang_is_the_last_resort() {
    assert_eq!(
        Language::of_file(Path::new("scripts/release"), "#!/usr/bin/env bash")
            .map_or("—", |l| l.name),
        "bash"
    );
    assert_eq!(
        Language::of_file(Path::new("scripts/tidy"), "#!/bin/sh").map_or("—", |l| l.name),
        "bash"
    );
    assert_eq!(
        Language::of_file(Path::new("hack"), "#!/usr/bin/env ruby").map_or("—", |l| l.name),
        "ruby"
    );
    assert_eq!(
        Language::of_file(Path::new("src/lib.rs"), "#!/usr/bin/env ruby").map_or("—", |l| l.name),
        "rust",
        "the name wins; the first line is only asked when it says nothing"
    );
    assert_eq!(
        Language::of_file(Path::new("notes"), "just some prose").map_or("—", |l| l.name),
        "—"
    );
}

/// The table is a table: every row can be registered, and no two rows
/// answer to the same name.
#[test]
fn every_row_is_whole() {
    let mut names: Vec<&str> = Vec::new();
    for language in Language::ALL {
        assert!(!language.name.is_empty());
        assert!(
            !language.grammar.is_empty(),
            "{} has no grammar",
            language.name
        );
        names.push(language.name);
    }
    let before = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(before, names.len(), "two rows share a name");
    assert!(
        Language::ALL.iter().any(|l| l.name == "rust"),
        "the languages asked for are in the table"
    );
}

/// A capture the table does not name falls back along its dots, and one
/// with no ancestor in the table is plain text, not a panic.
#[test]
fn a_capture_falls_back_along_its_dots() {
    assert_eq!(Role::of_capture("keyword"), Some(Role::Keyword));
    assert_eq!(Role::of_capture("function.method"), Some(Role::Function));
    // **An escape is a keyword's colour and a doc comment its own,
    // since M8.10** — both are rows of the table
    // rather than a fallback.
    assert_eq!(Role::of_capture("string.escape"), Some(Role::Keyword));
    assert_eq!(Role::of_capture("string.special"), Some(Role::String));
    assert_eq!(Role::of_capture("comment.doc"), Some(Role::Comment));
    assert_eq!(Role::of_capture("comment_doc"), Some(Role::Doc));
    assert_eq!(Role::of_capture("type"), Some(Role::Type));
    assert_eq!(Role::of_capture("boolean"), Some(Role::Number));
    assert_eq!(Role::of_capture("constant"), Some(Role::Constant));
    assert_eq!(Role::of_capture("title"), Some(Role::Function));
    assert_eq!(Role::of_capture("something.new"), None);
}

/// A file that is not text is said to be not text: NUL bytes, or bytes
/// that are not UTF-8. The answer is a kind, not an error.
#[test]
fn binary_is_recognised_before_it_is_parsed() {
    assert!(matches!(
        Text::of(b"\x89PNG\r\n\x1a\n\x00\x00"),
        Text::Binary
    ));
    assert!(matches!(Text::of(&[0xff, 0xfe, 0x41]), Text::Binary));
    assert!(matches!(
        Text::of("fn main() {}\n".as_bytes()),
        Text::Source { .. }
    ));
}

/// CRLF is read as LF and remembered, so that the file goes back the way
/// it came; a file with none is written back with none.
#[test]
fn line_endings_are_read_as_lf_and_remembered() {
    let Text::Source { text, line_ending } = Text::of("a\r\nb\r\n".as_bytes()) else {
        panic!("text");
    };
    assert_eq!(text, "a\nb\n");
    assert_eq!(line_ending, LineEnding::Crlf);
    assert_eq!(line_ending.apply("a\nb\n"), "a\r\nb\r\n");

    let Text::Source { text, line_ending } = Text::of("a\nb".as_bytes()) else {
        panic!("text");
    };
    assert_eq!(text, "a\nb");
    assert_eq!(line_ending, LineEnding::Lf);
    assert_eq!(line_ending.apply("a\nb"), "a\nb");
}

/// The capture table is one table. Every name it paints is a name the
/// component's theme is given, and no name is painted in one and
/// forgotten in the other — which is how the two of them had already
/// drifted apart by six names before this pass.
#[test]
fn the_capture_table_is_the_only_one() {
    let mut painted: Vec<String> = guild_desktop::highlight::CAPTURES
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    painted.sort();
    let themed = guild_desktop::editor::themed_captures();
    assert_eq!(
        painted, themed,
        "the theme handed to the component and the table must name the same captures"
    );
}

/// A capture with no slot in the component's forty is renamed as the
/// query is registered, because a name without a dot gets no fallback
/// either — `(escape_sequence) @escape` in rust is why every escape in
/// every string had been grey since M8.1.
#[test]
fn a_capture_with_no_slot_is_renamed_in_the_query() {
    use guild_desktop::highlight::renamed_captures;
    assert_eq!(
        renamed_captures("(escape_sequence) @escape"),
        "(escape_sequence) @string.escape"
    );
    assert_eq!(renamed_captures("(alias) @module"), "(alias) @type");
    assert_eq!(
        renamed_captures("[\"ifeq\"] @conditional"),
        "[\"ifeq\"] @keyword"
    );
    assert_eq!(
        renamed_captures("(x) @string.escape"),
        "(x) @string.escape",
        "a name that already has a slot is left alone"
    );
    assert_eq!(
        renamed_captures("((word) @clean (#match? @clean \"x\"))"),
        "((word) @clean (#match? @clean \"x\"))",
        "a capture a predicate refers to is not a colour and is not touched"
    );
    assert_eq!(
        renamed_captures("(x) @escaped"),
        "(x) @escaped",
        "a longer name that merely starts with one is not the same name"
    );
}

/// A name is matched **along its dots**, so a renamed root carries its
/// children with it.
///
/// `@module` was renamed from M8.1.1 and `@module.builtin` was not — php
/// says the second and swift says `@character.special`, and both were
/// falling back through a root that exists in `RENAMED` and in no table
/// either engine reads. The letters are not what is matched: `@escaped`
/// is still not `@escape`, and `@keyword.repeat` is not what `@repeat`
/// is.
#[test]
fn a_renamed_root_carries_the_names_under_it() {
    use guild_desktop::highlight::renamed_captures;
    assert_eq!(
        renamed_captures("(x) @module.builtin"),
        "(x) @type.builtin",
        "php's builtin modules"
    );
    assert_eq!(
        renamed_captures("(x) @character.special"),
        "(x) @string.special",
        "swift's"
    );
    // **A built-in type is a keyword since M8.10, and a macro is a
    // macro** — in both engines, because the query says the component's
    // own names: as rows of the table they were a keyword and a macro in a
    // diff and, through the component's fallback, a type and a function in
    // a tab.
    assert_eq!(renamed_captures("(x) @type.builtin"), "(x) @keyword");
    assert_eq!(Role::of_capture("keyword"), Some(Role::Keyword));
    assert_eq!(renamed_captures("(x) @function.special"), "(x) @preproc");
    assert_eq!(Role::of_capture("preproc"), Some(Role::Macro));
    assert_eq!(
        renamed_captures("(x) @method.call"),
        "(x) @function.method.call",
        "lua's, which both engines then resolve to a function"
    );
    assert_eq!(
        renamed_captures("(x) @keyword.repeat"),
        "(x) @keyword.repeat",
        "the root is `keyword`, which nothing renames — `repeat` is not a prefix"
    );
    assert_eq!(
        renamed_captures("(x) @escaped.thing"),
        "(x) @escaped.thing",
        "a dot does not turn a longer name into a shorter one"
    );
}

/// **A `@` inside a string literal is a token of the language, not a
/// capture of the query.**
///
/// css matches its at-rules by name — `"@namespace" @keyword` — and scss
/// matches fourteen of them, `"@include"` among them. Both collide with
/// rows of `RENAMED`, and a rename that read the query as bytes turned
/// them into `"@type"` and `"@keyword"`: two queries looking for tokens
/// no stylesheet contains, and two at-rules going grey with nothing said
/// anywhere. Found by reading the fourteen queries before their rows were
/// written, which is the only place it could have been found.
#[test]
fn an_at_sign_inside_a_string_is_a_token_and_is_left_alone() {
    use guild_desktop::highlight::renamed_captures;
    assert_eq!(
        renamed_captures(r#""@namespace" @keyword"#),
        r#""@namespace" @keyword"#,
        "css's at-rule"
    );
    assert_eq!(
        renamed_captures(r#"["@include" "@extend"] @keyword.import"#),
        r#"["@include" "@extend"] @keyword.import"#,
        "scss's"
    );
    assert_eq!(
        renamed_captures(r#"(#match? @escape "\\@") @escape"#),
        r#"(#match? @string.escape "\\@") @string.escape"#,
        "an escape inside the string does not end it early, and the captures outside it still move"
    );
}

/// The four names the fourteen new grammars use for something the table
/// already has a colour for, and the one it deliberately leaves grey.
///
/// `field` and `method` are the two that carry weight: a Lua file is
/// mostly `a.b` and `obj:method()`, and both were drawing as plain text
/// with nothing logged.
#[test]
fn a_capture_no_slot_can_hold_is_given_one_that_can() {
    use guild_desktop::highlight::{Paint, paint_of_capture, renamed_captures, unpainted};

    let renamed = |name: &str| {
        let text = renamed_captures(&format!("(x) @{name}"));
        text.trim_start_matches("(x) @").to_owned()
    };
    assert_eq!(paint_of_capture(&renamed("field")), Some(Paint::Plain));
    assert_eq!(
        paint_of_capture(&renamed("method")),
        Some(Paint::Role(Role::Function))
    );
    assert_eq!(
        paint_of_capture(&renamed("method.call")),
        Some(Paint::Role(Role::Function))
    );
    assert_eq!(
        paint_of_capture(&renamed("storageclass")),
        Some(Paint::Role(Role::Keyword))
    );
    assert_eq!(paint_of_capture(&renamed("delimiter")), Some(Paint::Quiet));

    // XML's `(CharData) @markup` is the text between the tags. Prose has
    // no colour in section 2.4, and the three names under it inherit the
    // decision rather than each making it again.
    for name in ["markup", "markup.heading", "markup.raw", "markup.link"] {
        assert!(unpainted(name), "@{name}");
    }
}

/// **A vendored query keeps its licence and its provenance beside it.**
///
/// `include_str!` is already the guard that the query itself is there —
/// the crate does not compile without it. Nothing checks the two files
/// that make the copy legitimate, and those are the ones a person adding
/// the next grammar by hand would leave out: the licence, because MIT and
/// Apache-2.0 both require the notice to travel with the code, and the
/// provenance, because a `.scm` with no origin cannot be regenerated or
/// audited. Just now uses its published query; every remaining copy is checked.
///
/// Here rather than in a file of its own so that `tests/all/main.rs`,
/// which five other branches of M8.8 are also editing, keeps the one line
/// it had.
#[test]
fn every_vendored_query_travels_with_its_licence() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/queries");
    for name in ["graphql", "hcl", "proto"] {
        assert!(dir.join(name).is_dir(), "missing vendored query: {name}");
    }
    for entry in std::fs::read_dir(&dir).expect("the vendored queries") {
        let grammar = entry.expect("an entry").path();
        if !grammar.is_dir() {
            continue;
        }
        for file in ["highlights.scm", "LICENSE", "PROVENANCE"] {
            assert!(
                grammar.join(file).is_file(),
                "{}: no {file}",
                grammar.display()
            );
        }
        let licence = std::fs::read_to_string(grammar.join("LICENSE")).expect("the licence");
        assert!(
            licence.contains("MIT License")
                || licence.contains("Permission is hereby granted, free of charge")
                || licence.contains("Apache License"),
            "{}: the licence reads as neither MIT nor Apache-2.0",
            grammar.display()
        );
    }
}

/// M8.5 fills the column M8.1.1 left empty: a language's server is its
/// command and that command's arguments, which is why it is a slice and
/// not a name — `typescript-language-server` needs `--stdio` and
/// `rust-analyzer` needs nothing.
#[test]
fn a_language_names_the_server_that_reads_it() {
    let rust = Language::of(std::path::Path::new("src/lib.rs")).expect("rust is in the table");

    assert_eq!(rust.server.first(), Some(&"rust-analyzer"));

    // Each of these was asked for a definition in a fixture of its own
    // before its row was written, on 2026-09-17: a project of two files,
    // the name in one and its definition in the other.
    for (file, server) in [
        ("main.go", &["gopls"][..]),
        ("lib/greeter.rb", &["ruby-lsp"][..]),
        ("src/Main.java", &["jdtls"][..]),
        ("src/Main.kt", &["kotlin-lsp", "--stdio"][..]),
        ("build.gradle.kts", &["kotlin-lsp", "--stdio"][..]),
        ("lib/app.ex", &["expert", "--stdio"][..]),
        // Asked the same way on 2026-09-28, in a SwiftPM package.
        ("Sources/Hello/main.swift", &["sourcekit-lsp"][..]),
        ("Package.swift", &["sourcekit-lsp"][..]),
    ] {
        let language = Language::of(std::path::Path::new(file)).expect("in the table");
        assert_eq!(language.server, server, "{file}");
    }

    let yaml = Language::of(std::path::Path::new("compose.yaml")).expect("yaml is in the table");
    assert!(
        yaml.server.is_empty(),
        "a language with no server is not an error, and says so by being empty"
    );
}

/// Whatever a row names must be startable: a command with a space in it
/// is a shell line, and nothing here runs a shell — that is M8.4's
/// business, and a server started through one could not be killed by its
/// group.
#[test]
fn every_server_in_the_table_is_a_command_and_its_arguments() {
    for language in Language::ALL {
        let Some(command) = language.server.first() else {
            continue;
        };
        assert!(
            !command.contains(char::is_whitespace),
            "{}'s server is a line rather than a command: {command}",
            language.name
        );
        assert!(
            !command.is_empty(),
            "{} names an empty command",
            language.name
        );
    }
}

/// **A server that knows only a described project is started only in
/// one.** sourcekit-lsp in an Xcode project compiled each file
/// alone and published forty errors a file — *cannot find 'CommandStore'
/// in scope* — where no server had said nothing; with a `buildServer.json`
/// beside the project it answered across files and published none. So
/// Swift's server starts where the project's root holds `Package.swift`
/// or `buildServer.json`, and a language that names no such file starts
/// anywhere, as before.
#[test]
fn a_server_that_needs_a_described_project_starts_only_in_one() {
    let swift = Language::of(Path::new("main.swift")).expect("swift is in the table");
    assert_eq!(
        swift.server_needs,
        &["Package.swift", "buildServer.json"][..]
    );

    let xcode = Scratch::new("guild-xcode-");
    std::fs::create_dir_all(xcode.join("App.xcodeproj")).expect("a project");
    assert!(!swift.server_starts_in(&xcode));

    std::fs::write(xcode.join("buildServer.json"), "{}").expect("a bridge");
    assert!(swift.server_starts_in(&xcode));

    let package = Scratch::new("guild-swiftpm-");
    std::fs::write(package.join("Package.swift"), "").expect("a manifest");
    assert!(swift.server_starts_in(&package));

    // The root's own, not a directory of that name, and not a nested
    // package's: the server is started in the project's root and reads
    // what is there.
    let nested = Scratch::new("guild-nested-");
    std::fs::create_dir_all(nested.join("Package.swift")).expect("a directory");
    std::fs::create_dir_all(nested.join("Tools/Lint")).expect("a directory");
    std::fs::write(nested.join("Tools/Lint/Package.swift"), "").expect("a manifest");
    assert!(!swift.server_starts_in(&nested));

    let rust = Language::of(Path::new("lib.rs")).expect("rust is in the table");
    assert!(rust.server_needs.is_empty());
    assert!(
        rust.server_starts_in(&nested),
        "a server that needs nothing"
    );
}

/// **A dependency's root is the nearest ancestor holding the language's
/// manifest.** M8.8.3, request 10 — one rule for every language, which
/// is what makes `manifest` a column of this table rather than a
/// function per language.
///
/// The two shapes were measured, not guessed: cargo unpacks a crate as
/// `…/index.crates.io-<hash>/serde-1.0.228/` with a `Cargo.toml` in it,
/// and Go puts the version in the module directory's own name,
/// `…/pkg/mod/gopkg.in/yaml.v3@v3.0.1/`, with a `go.mod`. The same walk
/// answers both.
#[test]
fn a_dependency_root_is_the_nearest_ancestor_with_the_manifest() {
    let dir = Scratch::new("guild-manifest-");
    let crate_root = dir.join("index.crates.io-1949cf8c/serde-1.0.228");
    std::fs::create_dir_all(crate_root.join("src/de")).expect("a directory");
    std::fs::write(crate_root.join("Cargo.toml"), "[package]\n").expect("a manifest");
    std::fs::write(crate_root.join("src/de/mod.rs"), "").expect("a file");

    let rust = Language::of(Path::new("mod.rs")).expect("rust is in the table");
    assert_eq!(
        guild_desktop::highlight::dependency_root(&crate_root.join("src/de/mod.rs"), rust),
        Some(crate_root.clone()),
        "the directory a person calls serde 1.0.228"
    );

    // A file with no manifest anywhere above it belongs to no
    // dependency, and saying so is the answer — not a root invented from
    // the parent directory.
    let loose = dir.join("loose.rs");
    std::fs::write(&loose, "").expect("a file");
    assert_eq!(
        guild_desktop::highlight::dependency_root(&loose, rust),
        None
    );

    // A language whose dependencies are jars names no manifest, so the
    // walk never starts.
    let kotlin = Language::of(Path::new("a.kt")).expect("kotlin is in the table");
    assert!(
        kotlin.manifest.is_empty(),
        "we decompile nothing, so kotlin has no source root to find"
    );
    assert_eq!(
        guild_desktop::highlight::dependency_root(&crate_root.join("src/de/mod.rs"), kotlin),
        None
    );
}

/// The indent a language's own tool writes, by the language's name.
fn indent_of(name: &str) -> Indent {
    Language::ALL
        .iter()
        .find(|language| language.name == name)
        .unwrap_or_else(|| panic!("{name} is a row of the table"))
        .indent
}

/// **A Makefile recipe line must begin with a tab**, and `gofmt` writes
/// nothing else; YAML forbids a tab in its indentation outright. Tab
/// wrote two spaces into all three before M5.11 — `make` rejects that
/// recipe with *missing separator*.
#[test]
fn go_and_make_indent_with_tabs_and_yaml_never_does() {
    assert_eq!(indent_of("make"), Indent::Tabs);
    assert_eq!(indent_of("go"), Indent::Tabs);
    assert_ne!(indent_of("yaml"), Indent::Tabs);
    assert_eq!(
        Language::of(Path::new("Makefile")).map(|language| language.indent),
        Some(Indent::Tabs),
        "and a Makefile reaches that row by its name"
    );
}

/// Two, four, or a tab: no language writes three or eight, and a width
/// outside those is a typo in the table rather than a style.
#[test]
fn every_language_indents_by_two_four_or_a_tab() {
    for language in Language::ALL {
        assert!(
            matches!(
                language.indent,
                Indent::Spaces(2) | Indent::Spaces(4) | Indent::Tabs
            ),
            "{} indents by {:?}",
            language.name,
            language.indent
        );
    }
    assert_eq!(PLAIN_INDENT, Indent::Spaces(4), "the common default");
}

/// M5.11's decision 9, a row from each of its groups: four where
/// nothing else is said, two where the formatter every file of the
/// language passes through writes two.
#[test]
fn the_widths_are_the_ones_decision_9_names() {
    assert_eq!(indent_of("rust"), Indent::Spaces(4), "rustfmt");
    assert_eq!(indent_of("ruby"), Indent::Spaces(2), "RuboCop");
    assert_eq!(indent_of("typescript"), Indent::Spaces(2), "Prettier");
    assert_eq!(indent_of("python"), Indent::Spaces(4), "PEP 8");
    assert_eq!(indent_of("hcl"), Indent::Spaces(2), "terraform fmt");
    let two = [
        "ruby",
        "elixir",
        "javascript",
        "typescript",
        "tsx",
        "json",
        "jsonc",
        "yaml",
        "markdown",
        "html",
        "css",
        "scss",
        "svelte",
        "graphql",
        "nix",
        "hcl",
        "bash",
        "proto",
    ];
    for name in two {
        assert_eq!(indent_of(name), Indent::Spaces(2), "{name}");
    }
    for language in Language::ALL {
        if !two.contains(&language.name) && !["make", "go"].contains(&language.name) {
            assert_eq!(
                language.indent,
                Indent::Spaces(4),
                "{} is not named by decision 9, so it is four",
                language.name
            );
        }
    }
}

/// **A grammar may ask for a role of its own** — M8.10's layer, and the
/// reason is Go: its palette draws a type *reference* teal, a package
/// olive and a built-in in a second orange, while Java and Kotlin draw a
/// type plain. One mapping cannot be both, so the mapping takes the
/// grammar as well as the capture.
#[test]
fn a_grammar_may_paint_a_capture_its_own_way() {
    use guild_desktop::highlight::{Paint, Role, paint_of};

    assert_eq!(
        paint_of("go", "type"),
        Some(Paint::Role(Role::TypeRef)),
        "Go's type reference is the plugin's teal"
    );
    assert_eq!(
        paint_of("kotlin", "type"),
        Some(Paint::Role(Role::Type)),
        "every other grammar keeps the table's own row"
    );
    assert_eq!(
        paint_of("go", "constant.builtin"),
        Some(Paint::Role(Role::Builtin))
    );
    assert_eq!(
        paint_of("go", "function.builtin"),
        Some(Paint::Role(Role::Builtin))
    );
    assert_eq!(paint_of("go", "module"), Some(Paint::Role(Role::Namespace)));
    // An override falls back along the dots like any row of the table,
    // and a capture the grammar says nothing about is the table's.
    assert_eq!(paint_of("go", "keyword"), Some(Paint::Role(Role::Keyword)));
    assert_eq!(paint_of("go", "comment"), Some(Paint::Role(Role::Comment)));
}

/// A language that rides another's grammar cannot have paints of its own:
/// both engines are keyed by grammar, so two languages on one grammar
/// with two tables would be a colour that depends on which row was found
/// first.
#[test]
fn no_two_grammars_share_a_paint_table() {
    use guild_desktop::highlight::PAINTS;

    let mut names: Vec<&str> = PAINTS.iter().map(|(grammar, _)| *grammar).collect();
    let before = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(before, names.len(), "two rows of PAINTS name one grammar");
    for (grammar, rows) in PAINTS {
        assert!(
            Language::ALL.iter().any(|l| l.grammar == *grammar),
            "{grammar} is not a grammar any language names"
        );
        assert!(!rows.is_empty(), "{grammar} has an empty paint table");
    }
}
