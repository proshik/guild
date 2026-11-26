//! Symbols from tags and from a server, placed in characters — M8.9 group
//! B's tasks 10 and 11. Declared by the seams commit, which gives every
//! file of the pass its place in `main.rs`.

use std::path::PathBuf;

use guild_desktop::symbols::{self, Source, Symbol, SymbolKind, Where};

/// The names, kinds and lines a text's tags give, in the order found.
fn tagged(text: &str, grammar: &str) -> Vec<(String, SymbolKind, u32)> {
    symbols::of_text(text, grammar)
        .into_iter()
        .map(|symbol| (symbol.name, symbol.kind, symbol.line))
        .collect()
}

#[test]
fn every_grammar_that_ships_a_tags_query_uses_it() {
    let mut with_tags: Vec<&str> = guild_desktop::grammars::all()
        .iter()
        .filter(|grammar| !grammar.tags.is_empty())
        .map(|grammar| grammar.name)
        .collect();
    with_tags.sort_unstable();
    assert_eq!(
        with_tags,
        vec![
            "c",
            "cpp",
            "csharp",
            "elixir",
            "go",
            "java",
            "javascript",
            "lua",
            "php",
            "properties",
            "python",
            "ruby",
            "rust",
            "swift",
            "tsx",
            "typescript",
        ],
        "task 0's sixteen: fifteen crates, tsx riding typescript's"
    );
    for name in with_tags {
        assert!(
            symbols::configuration(name).is_some(),
            "{name}'s tags compile into a configuration"
        );
    }
    // C#'s query is refused whole as published (`InvalidCapture("module")`)
    // and compiles with its one `@module` pattern taken out: what is left
    // still finds its classes and methods.
    let csharp = tagged("namespace N { class A { void M() {} } }\n", "csharp");
    assert!(
        csharp.contains(&("A".to_owned(), SymbolKind::Class, 0))
            && csharp.contains(&("M".to_owned(), SymbolKind::Method, 0)),
        "{csharp:?}"
    );
    assert!(
        symbols::configuration("toml").is_none(),
        "a grammar with no tags has no configuration"
    );
}

#[test]
fn typescript_takes_javascript_s_tags_too() {
    let sample = "function greet() {}\ninterface Person {}\n";
    for grammar in ["typescript", "tsx"] {
        let found = tagged(sample, grammar);
        assert!(
            found.contains(&("greet".to_owned(), SymbolKind::Function, 0)),
            "{grammar}: {found:?}"
        );
        assert!(
            found.contains(&("Person".to_owned(), SymbolKind::Interface, 1)),
            "{grammar}: {found:?}"
        );
    }
}

#[test]
fn a_rust_file_gives_its_functions_methods_structs_and_traits() {
    let sample = "\
struct Point { x: i32 }
enum Shape { A }
trait Draw { fn draw(&self); }
impl Draw for Point { fn draw(&self) {} }
fn main() {}
mod inner {}
macro_rules! shout { () => {} }
";
    let mut found = tagged(sample, "rust");
    found.sort_by_key(|(name, _, line)| (*line, name.clone()));
    assert_eq!(
        found,
        vec![
            ("Point".to_owned(), SymbolKind::Class, 0),
            ("Shape".to_owned(), SymbolKind::Class, 1),
            ("Draw".to_owned(), SymbolKind::Interface, 2),
            ("draw".to_owned(), SymbolKind::Method, 3),
            ("main".to_owned(), SymbolKind::Function, 4),
            ("inner".to_owned(), SymbolKind::Module, 5),
            ("shout".to_owned(), SymbolKind::Macro, 6),
        ],
        "definitions only: the impl's own names are references"
    );
}

#[test]
fn a_symbol_is_placed_at_its_name_counted_in_characters_from_zero() {
    let found = symbols::of_text("/* ёж */ fn greet() {}\n", "rust");
    assert_eq!(found.len(), 1);
    let greet: &Symbol = &found[0];
    assert_eq!(greet.name, "greet");
    assert_eq!(
        (greet.line, greet.column),
        (0, 12),
        "not the byte column 14"
    );
    assert_eq!(greet.container, None);

    assert!(
        symbols::of_text("fn a() {}\n", "no-such-grammar").is_empty(),
        "a name no grammar takes gives nothing"
    );
}

#[test]
fn a_file_over_the_limit_gives_nothing() {
    let line = "fn a() {}\n";
    let lines = guild_desktop::file_tab::HIGHLIGHT_LIMIT as usize / line.len() + 1;
    let text = line.repeat(lines);
    assert!(text.len() as u64 > guild_desktop::file_tab::HIGHLIGHT_LIMIT);
    assert!(symbols::of_text(&text, "rust").is_empty());
}

#[test]
fn ranking_is_by_the_name_not_the_path() {
    let at = |path: &str| Where::Project {
        path: PathBuf::from(path),
        line: 3,
        column: 4,
    };
    let symbol = |name: &str| Symbol {
        name: name.to_owned(),
        kind: SymbolKind::Function,
        container: None,
        line: 3,
        column: 4,
    };
    let all = vec![
        (at("src/greet.rs"), symbol("other")),
        (at("src/x.rs"), symbol("greet")),
        (at("src/y.rs"), symbol("grxext")),
    ];

    let rows = symbols::rank("greet", &all, 50);

    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, vec!["greet"], "a path is not a name");
    assert_eq!(rows[0].at, at("src/x.rs"));
    assert_eq!(rows[0].marks, vec![0, 1, 2, 3, 4]);
    assert_eq!(rows[0].from, Source::Tags);

    let rows = symbols::rank("gre", &all, 1);
    assert_eq!(rows.len(), 1, "the limit holds");
    assert_eq!(rows[0].name, "greet");
    assert!(
        symbols::rank("", &all, 50).is_empty(),
        "nothing typed asks nothing"
    );
}

#[test]
fn both_shapes_of_the_answer_are_read() {
    let information = serde_json::json!([{
        "name": "put_fields_away",
        "kind": 12,
        "location": {
            "uri": "file:///work/guild/src/root/recent.rs",
            "range": {"start": {"line": 153, "character": 17}, "end": {"line": 153, "character": 32}}
        },
        "containerName": "Root"
    }, {
        "name": "Root",
        "kind": 23,
        "location": {
            "uri": "file:///work/My%20Guild/lib.rs",
            "range": {"start": {"line": 120, "character": 11}, "end": {"line": 120, "character": 15}}
        }
    }]);
    let found = symbols::from_server(information);
    assert_eq!(found.len(), 2);
    let (path, line, utf16, symbol) = &found[0];
    assert_eq!(path, &PathBuf::from("/work/guild/src/root/recent.rs"));
    assert_eq!((*line, *utf16), (153, 17));
    assert_eq!(symbol.name, "put_fields_away");
    assert_eq!(symbol.kind, SymbolKind::Function);
    assert_eq!(symbol.container.as_deref(), Some("Root"));
    let (path, _, _, symbol) = &found[1];
    assert_eq!(
        path,
        &PathBuf::from("/work/My Guild/lib.rs"),
        "a URI is decoded"
    );
    assert_eq!(
        (symbol.kind, symbol.container.as_deref()),
        (SymbolKind::Struct, None)
    );

    let workspace = serde_json::json!([{
        "name": "Walker",
        "kind": 5,
        "location": {"uri": "file:///work/guild/src/walk.rs"},
        "containerName": ""
    }, {
        "name": "elsewhere",
        "kind": 12,
        "location": {"uri": "jar:file:///x.jar!/A.class"}
    }]);
    let found = symbols::from_server(workspace);
    assert_eq!(found.len(), 1, "a URI that is not a file is no place");
    let (path, line, utf16, symbol) = &found[0];
    assert_eq!(path, &PathBuf::from("/work/guild/src/walk.rs"));
    assert_eq!(
        (*line, *utf16),
        (0, 0),
        "a location with only a URI is placed at its top"
    );
    assert_eq!(symbol.kind, SymbolKind::Class);
    assert_eq!(symbol.container, None, "an empty container is none");

    assert!(symbols::from_server(serde_json::Value::Null).is_empty());
}

#[test]
fn a_server_row_replaces_the_tag_at_the_same_place() {
    let at = |line: u32| Where::Project {
        path: PathBuf::from("src/lib.rs"),
        line,
        column: 11,
    };
    let row = |name: &str, kind: SymbolKind, line: u32, from: Source| symbols::Row {
        name: name.to_owned(),
        kind,
        container: None,
        at: at(line),
        marks: vec![0],
        from,
    };
    let tags = vec![
        row("Root", SymbolKind::Class, 120, Source::Tags),
        row("Rooted", SymbolKind::Function, 200, Source::Tags),
    ];
    let mut from_server = row("Root", SymbolKind::Struct, 120, Source::Server);
    from_server.container = Some("guild_desktop".to_owned());
    let server = vec![
        from_server.clone(),
        row("RootOnly", SymbolKind::Function, 300, Source::Server),
    ];

    let merged = symbols::merge(tags, server);

    assert_eq!(merged.len(), 3, "{merged:?}");
    assert_eq!(
        merged[0], from_server,
        "the server's row, in the tag's place"
    );
    assert_eq!(merged[1].name, "Rooted");
    assert_eq!(merged[2].name, "RootOnly");

    let untouched = vec![row("Root", SymbolKind::Class, 120, Source::Tags)];
    assert_eq!(
        symbols::merge(untouched.clone(), Vec::new()),
        untouched,
        "a ready server answering `[]` is no rows yet, and hides nothing"
    );
}

#[test]
fn a_symbol_outside_every_root_is_outside() {
    let project = std::path::Path::new("/work/guild");
    let libraries = [PathBuf::from("/registry/serde-1.0.228")];

    assert_eq!(
        symbols::where_of(
            project,
            &libraries,
            std::path::Path::new("/work/guild/src/lib.rs"),
            3,
            4
        ),
        Where::Project {
            path: PathBuf::from("src/lib.rs"),
            line: 3,
            column: 4
        }
    );
    assert_eq!(
        symbols::where_of(
            project,
            &libraries,
            std::path::Path::new("/registry/serde-1.0.228/src/de/mod.rs"),
            5,
            6
        ),
        Where::Library {
            under: 0,
            path: PathBuf::from("src/de/mod.rs"),
            line: 5,
            column: 6
        }
    );
    assert_eq!(
        symbols::where_of(
            project,
            &libraries,
            std::path::Path::new("/toolchain/lib/rustlib/src/libunwind.rs"),
            7,
            8
        ),
        Where::Outside {
            path: PathBuf::from("/toolchain/lib/rustlib/src/libunwind.rs"),
            line: 7,
            column: 8
        }
    );
    assert!(
        matches!(
            symbols::where_of(
                project,
                &libraries,
                std::path::Path::new("/work/guild-other/a.rs"),
                0,
                0
            ),
            Where::Outside { .. }
        ),
        "a sibling whose name starts with the project's is not in it"
    );
}

#[test]
fn a_utf16_column_is_counted_in_characters() {
    let text = "fn a() {}\n/* 🦀 */ fn greet() {}\n";
    // `greet` is 12 UTF-16 units in: the crab is two of them.
    assert_eq!(symbols::placed(text, 1, 12), 11);
    assert_eq!(symbols::placed(text, 0, 3), 3, "ASCII is the same count");
    assert_eq!(
        symbols::placed(text, 9, 4),
        0,
        "a line past the end is its top"
    );
}

#[test]
fn a_server_row_whose_range_starts_before_the_name_replaces_the_tag_on_its_line() {
    let at = |line: u32, column: u32| Where::Project {
        path: PathBuf::from("src/lib.rs"),
        line,
        column,
    };
    let row = |name: &str, at: Where, from: Source| symbols::Row {
        name: name.to_owned(),
        kind: SymbolKind::Struct,
        container: None,
        at,
        marks: vec![0],
        from,
    };
    // `pub struct Root` — a tag is placed at the name, and a server whose
    // range is the whole item starts at `pub`.
    let tags = vec![row("Root", at(120, 11), Source::Tags)];
    let whole = row("Root", at(120, 0), Source::Server);
    let other = row("new", at(120, 0), Source::Server);

    let merged = symbols::merge(tags.clone(), vec![whole.clone()]);
    let mut at_the_name = whole;
    at_the_name.at = at(120, 11);
    assert_eq!(
        merged,
        vec![at_the_name],
        "one symbol, not two rows of it — the server's, landing where the tag found the name"
    );

    let merged = symbols::merge(tags.clone(), vec![other.clone()]);
    assert_eq!(
        merged,
        vec![tags[0].clone(), other],
        "another name on the same line is another symbol"
    );
}

#[test]
fn a_tie_is_broken_by_the_shorter_name_as_it_is_read() {
    let at = Where::Project {
        path: PathBuf::from("src/lib.rs"),
        line: 0,
        column: 0,
    };
    let symbol = |name: &str| Symbol {
        name: name.to_owned(),
        kind: SymbolKind::Function,
        container: None,
        line: 0,
        column: 0,
    };
    // Four letters in four bytes, and three letters in five.
    let all = vec![(at.clone(), symbol("xabc")), (at.clone(), symbol("xéé"))];

    let names = |rows: &[symbols::Row]| -> Vec<String> {
        rows.iter().map(|row| row.name.clone()).collect()
    };
    let ranked = symbols::rank("x", &all, 50);
    assert_eq!(names(&ranked), vec!["xéé", "xabc"]);
    let reranked = symbols::rerank("x", ranked.into_iter().rev().collect(), 50);
    assert_eq!(names(&reranked), vec!["xéé", "xabc"]);
}
