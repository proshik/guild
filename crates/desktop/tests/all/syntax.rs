//! The spans of a text: what the diff, Annotate and a commit's files are
//! coloured with. Out here because they reach it through the public door,
//! and because it must answer without a window.

use guild_desktop::highlight::{Paint, Role};
use guild_desktop::syntax;

/// The paints of one line, as `(text, paint)` pairs, so that a case reads
/// as what a person would see rather than as byte offsets.
fn painted(text: &str, grammar: &str, line: usize) -> Vec<(String, Option<Paint>)> {
    let rows = syntax::spans(text, grammar);
    let source = text.lines().nth(line).expect("that line");
    rows[line]
        .iter()
        .map(|span| (source[span.at.clone()].to_owned(), span.paint))
        .collect()
}

/// A line is cut into runs, and every byte of it is in exactly one run —
/// a gap would be a hole in the drawn line, not an absence of colour.
#[test]
fn a_line_is_covered_from_end_to_end() {
    let text = "fn main() {}\n";
    let rows = syntax::spans(text, "rust");
    assert_eq!(rows.len(), 1);
    let mut at = 0;
    for span in &rows[0] {
        assert_eq!(span.at.start, at, "a gap or an overlap");
        at = span.at.end;
    }
    assert_eq!(at, "fn main() {}".len(), "the line is covered to its end");
}

/// The seven roles reach the spans: a keyword is a keyword and a string
/// is a string.
#[test]
fn the_roles_reach_the_text() {
    let runs = painted("fn main() {}\n", "rust", 0);
    assert!(
        runs.iter()
            .any(|(text, paint)| text == "fn" && *paint == Some(Paint::Role(Role::Keyword))),
        "{runs:?}"
    );
    let runs = painted("let s = \"hello\";\n", "rust", 0);
    assert!(
        runs.iter().any(
            |(text, paint)| text.contains("hello") && *paint == Some(Paint::Role(Role::String))
        ),
        "{runs:?}"
    );
}

/// A capture inside another wins over it — the component's own rule, and
/// the reason the two engines agree.
///
/// **Since M8.10 the escape is a keyword's colour**, so the
/// win is visible: the literal is cut into three runs and the middle one
/// is the escape. Until then both were the string's colour and the proof
/// was the opposite — one run, because an unpainted escape would have cut
/// the literal the same way. The claim is the same either way: the inner
/// capture wins, and nothing in the literal goes unpainted.
#[test]
fn a_capture_inside_another_wins_over_it() {
    let runs = painted("let s = \"a\\nb\";\n", "rust", 0);
    let inside: Vec<_> = runs
        .iter()
        .skip_while(|(text, _)| !text.starts_with('"'))
        .take_while(|(text, _)| !text.starts_with(';'))
        .collect();
    let text: String = inside.iter().map(|(text, _)| text.as_str()).collect();
    assert_eq!(
        text, "\"a\\nb\"",
        "nothing of the literal is unpainted: {runs:?}"
    );
    assert_eq!(
        inside
            .iter()
            .map(|(text, paint)| (text.as_str(), *paint))
            .collect::<Vec<_>>(),
        vec![
            ("\"a", Some(Paint::Role(Role::String))),
            ("\\n", Some(Paint::Role(Role::Keyword))),
            ("b\"", Some(Paint::Role(Role::String))),
        ],
        "the escape is a run of its own, in the keyword's colour: {runs:?}"
    );
}

/// A text with no grammar is not an error: it is a text with no colour,
/// one empty row per line.
#[test]
fn a_text_with_no_grammar_has_no_colour() {
    let text = "one\ntwo\nthree\n";
    let rows = syntax::spans(text, "text");
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row.is_empty()));

    let rows = syntax::spans(text, "a-language-nobody-registers");
    assert_eq!(rows.len(), 3, "an unknown grammar is not fewer lines");
    assert!(rows.iter().all(|row| row.is_empty()));
}

/// A span never crosses a line: the views draw one line at a time and
/// would have to cut it themselves otherwise.
#[test]
fn a_span_never_crosses_a_line() {
    let text = "/* a comment\n   over two lines */\nfn main() {}\n";
    let rows = syntax::spans(text, "rust");
    assert_eq!(rows.len(), 3);
    for (number, line) in text.lines().enumerate() {
        for span in &rows[number] {
            assert!(
                span.at.end <= line.len(),
                "line {number}: {span:?} runs past its line"
            );
        }
    }
    assert!(
        rows[1]
            .iter()
            .any(|span| span.paint == Some(Paint::Role(Role::Comment))),
        "the comment's second line is a comment too"
    );
}

/// The empty text is no lines, and a text of one empty line is one empty
/// row — the two are not the same and a view asking for row zero must
/// not be given a panic.
#[test]
fn an_empty_text_is_not_a_panic() {
    assert!(syntax::spans("", "rust").is_empty());
    assert_eq!(syntax::spans("\n", "rust").len(), 1);
}

/// Every span begins and ends on a character boundary. They are byte
/// offsets, and a Cyrillic comment in a Rust file is where a naive cut
/// would land inside a character — which GPUI refuses when it builds the
/// runs, so the window would go down on the first such file drawn.
#[test]
fn a_span_lands_on_character_boundaries() {
    let text = "// привет, мир\nfn main() { let s = \"тест\"; }\n";
    let rows = syntax::spans(text, "rust");
    for (number, line) in text.lines().enumerate() {
        for span in &rows[number] {
            assert!(
                line.is_char_boundary(span.at.start) && line.is_char_boundary(span.at.end),
                "line {number}: {span:?} cuts a character in {line:?}"
            );
            // And the slice itself must not panic, which is what a view does.
            let _ = &line[span.at.clone()];
        }
    }
}

/// Text that is not what the grammar expects is not an error: tree-sitter
/// parses what it can and calls the rest an error node, and the answer is
/// still one row per line.
#[test]
fn prose_parsed_as_code_is_not_a_panic() {
    let text = "this is not rust at all\nnor is this ((( \n";
    let rows = syntax::spans(text, "rust");
    assert_eq!(rows.len(), 2);
    for (number, line) in text.lines().enumerate() {
        let mut at = 0;
        for span in &rows[number] {
            assert_eq!(span.at.start, at);
            at = span.at.end;
        }
        assert_eq!(at, line.len(), "line {number} is covered to its end");
    }
}

/// **A grammar that extends another is drawn with both queries.**
///
/// `tree-sitter-typescript` publishes the TypeScript-specific half of its
/// highlights — thirty-five lines against JavaScript's two hundred — and
/// the half it inherits is published by `tree-sitter-javascript`. Nothing
/// says so out loud: `Query::new` answers `Ok` for a query that colours
/// almost nothing, which is why a TypeScript file went from M8.1 to M8.8
/// with its comments, its strings, its numbers and its function names
/// grey and no line in any log.
///
/// So the test is a count. Naming four roles and stopping there would
/// pass again the day a query loses ninety of its hundred patterns and
/// keeps ten.
#[test]
fn a_grammar_that_extends_another_is_drawn_with_both_queries() {
    const SAMPLE: &str = r#"// what a TypeScript file is made of
import { readFile } from "node:fs";

export function greet(name: string): string {
  const times = 3;
  if (name.length === 0) {
    return `hello, ${name}`;
  }
  return "hi";
}
"#;
    for grammar in ["typescript", "tsx"] {
        let rows = syntax::spans(SAMPLE, grammar);
        let painted = rows
            .iter()
            .flatten()
            .filter(|span| span.paint.is_some())
            .count();
        assert!(
            painted >= 20,
            "{grammar}: {painted} painted runs — the inherited half of the query is missing"
        );
        let roles: Vec<Option<Paint>> = rows.iter().flatten().map(|span| span.paint).collect();
        for role in [Role::Comment, Role::String, Role::Number, Role::Keyword] {
            assert!(
                roles.contains(&Some(Paint::Role(role))),
                "{grammar}: nothing is painted {role:?}, which only the inherited half paints"
            );
        }
    }
}

/// **The `<style>` block is coloured, and coloured the same as a
/// stylesheet.**
///
/// HTML's own highlights query is thirteen lines. Everything that makes
/// an HTML file look coloured is in the other query — `#set!
/// injection.language` handing `<script>` to javascript and `<style>` to
/// css — and the component follows those in a tab. Without them here, an
/// HTML file would be coloured in a tab and grey in a diff, in Annotate
/// and in a commit's files, which is the disagreement the module doc says
/// cannot happen.
///
/// The assertion is the same block twice: once inside the page and once
/// as a stylesheet of its own. The two must be painted identically, and
/// they are the same bytes, so the comparison is exact rather than a
/// count.
#[test]
fn an_injected_block_is_painted_as_its_own_language_would_paint_it() {
    const STYLE: &str = "/* a rule */\n.card { color: red; }\n";
    let page = format!("<!DOCTYPE html>\n<html>\n<style>\n{STYLE}</style>\n</html>\n");

    let inside = syntax::spans(&page, "html");
    let alone = syntax::spans(STYLE, "css");

    // The `<style>` block begins on the fourth line of the page.
    let offset = 3;
    for (number, row) in alone.iter().enumerate() {
        assert_eq!(
            &inside[number + offset],
            row,
            "line {number} of the block: inside the page against on its own"
        );
    }
    assert!(
        alone.iter().flatten().any(|span| span.paint.is_some()),
        "the comparison is worth nothing if css paints nothing"
    );
}

/// The script half of the same, and the proof that the outer grammar has
/// not simply been replaced: the tags around the block are still HTML's.
#[test]
fn a_page_keeps_its_own_colour_around_what_it_injects() {
    const PAGE: &str = "<html>\n<script>\nlet x = 1; // a comment\n</script>\n</html>\n";
    let rows = syntax::spans(PAGE, "html");

    let script = &rows[2];
    let line = PAGE.lines().nth(2).expect("the script line");
    let painted: Vec<(&str, Option<Paint>)> = script
        .iter()
        .map(|span| (&line[span.at.clone()], span.paint))
        .collect();
    assert!(
        painted
            .iter()
            .any(|(text, paint)| *text == "let" && *paint == Some(Paint::Role(Role::Keyword))),
        "javascript's keyword: {painted:?}"
    );
    assert!(
        painted
            .iter()
            .any(|(_, paint)| *paint == Some(Paint::Role(Role::Comment))),
        "javascript's comment: {painted:?}"
    );

    let tag = &rows[0];
    let line = PAGE.lines().next().expect("the first line");
    assert!(
        tag.iter()
            .any(|span| line[span.at.clone()] == *"html"
                && span.paint == Some(Paint::Role(Role::Tag))),
        "the tag around it is still HTML's: {tag:?}"
    );
}

/// An injection may only reach a grammar the outer one declares, and the
/// depth is capped — a query is free to say that a grammar injects
/// itself, and a text is not allowed to take for ever because it does.
#[test]
fn an_injection_that_goes_nowhere_is_not_an_error() {
    // php's query names phpdoc and a heredoc's own tag, and the row
    // declares no injections at all; the file is still coloured by php's
    // own query.
    let rows = syntax::spans("<?php\n// a comment\n$x = 1;\n", "php");
    assert!(
        rows[1]
            .iter()
            .any(|span| span.paint == Some(Paint::Role(Role::Comment))),
        "{:?}",
        rows[1]
    );
}

/// **Two captures on one node: the first is the colour, the rest are
/// annotations.**
///
/// The vendored queries mark a node for the spell checker by capturing it
/// twice — `(comment) @comment @spell` — and `@spell` is not a colour,
/// which is why it has been on the deliberately-grey list since M8.1.1.
/// Taking the last capture makes the annotation win and the comment go
/// grey; sql, scss, swift and graphql all write their comments that way,
/// so four of the languages this pass adds would have arrived with no
/// comment colour at all.
///
/// The component does not have this bug and the rule was supposed to be
/// copied from it: `match_styles` keeps the **previous** name when the
/// next capture covers exactly the same bytes, and only `unique_styles`
/// after it lets a strictly-inner capture win. The copy took the second
/// half.
#[test]
fn a_second_capture_on_the_same_node_does_not_take_the_colour_away() {
    for (grammar, text) in [
        ("sql", "-- a comment\nselect 1;\n"),
        ("scss", "// a comment\n.a { color: red; }\n"),
        ("swift", "// a comment\nlet x = 1\n"),
        ("graphql", "# a comment\nquery Hero { name }\n"),
    ] {
        let rows = syntax::spans(text, grammar);
        assert!(
            rows[0]
                .iter()
                .any(|span| span.paint == Some(Paint::Role(Role::Comment))),
            "{grammar}: the comment is grey — @spell took the node from @comment: {:?}",
            rows[0]
        );
    }
}

/// **A query from outside the package still has to paint.**
///
/// graphql and hcl publish a parser and no `queries/` directory at all,
/// so the query beside each is nvim-treesitter's, vendored. That it
/// compiles proves only that the node names exist — `Query::new` does
/// refuse an unknown node type — and a query can name real nodes and
/// still be the wrong query for this grammar's tree. What it has to do is
/// colour, so that is what is asked.
#[test]
fn a_vendored_query_colours_the_grammar_it_was_taken_for() {
    let graphql = syntax::spans(
        "# a comment\nquery Hero($episode: Episode) {\n  hero(episode: $episode) { name }\n}\n",
        "graphql",
    );
    let roles: Vec<Option<Paint>> = graphql.iter().flatten().map(|span| span.paint).collect();
    assert!(
        roles.contains(&Some(Paint::Role(Role::Comment))),
        "{graphql:?}"
    );
    assert!(roles.contains(&Some(Paint::Role(Role::Type))), "Episode");

    let hcl = syntax::spans(
        "# a comment\nresource \"aws_s3_bucket\" \"b\" {\n  count = 3\n  acl   = \"private\"\n}\n",
        "hcl",
    );
    let roles: Vec<Option<Paint>> = hcl.iter().flatten().map(|span| span.paint).collect();
    assert!(roles.contains(&Some(Paint::Role(Role::Comment))), "{hcl:?}");
    assert!(roles.contains(&Some(Paint::Role(Role::String))));
    assert!(roles.contains(&Some(Paint::Role(Role::Number))));
}

/// The same, for C++, which is the other half of decision 11 and was
/// never registered before this pass — so it is the rule applied to a new
/// row rather than a defect repaired.
///
/// `tree-sitter-cpp` publishes seventy lines: the templates, the
/// namespaces and `auto`. Everything a C++ file shares with C — the
/// comments, the preprocessor, the labels, the delimiters — is in
/// `tree-sitter-c`'s eighty-one and in no constant its own crate exports.
#[test]
fn cpp_is_drawn_with_cs_query_as_well_as_its_own() {
    const SAMPLE: &str = r#"// a comment, which only C's half paints
#include <vector>

namespace guild {
  template <typename T>
  int count(const std::vector<T> &items) {
    return static_cast<int>(items.size());
  }
}
"#;
    let rows = syntax::spans(SAMPLE, "cpp");
    let paints = |line: usize| -> Vec<(String, Option<Paint>)> {
        let source = SAMPLE.lines().nth(line).expect("that line");
        rows[line]
            .iter()
            .map(|span| (source[span.at.clone()].to_owned(), span.paint))
            .collect()
    };
    let comment = paints(0);
    assert!(
        comment
            .iter()
            .any(|(_, paint)| *paint == Some(Paint::Role(Role::Comment))),
        "the comment is C's pattern and C++'s query has none: {comment:?}"
    );
    let include = paints(1);
    assert!(
        include
            .iter()
            .any(|(text, paint)| text == "#include" && *paint == Some(Paint::Role(Role::Keyword))),
        "`#include` is C's, and C++'s query says nothing about it: {include:?}"
    );
    let roles: Vec<Option<Paint>> = rows.iter().flatten().map(|span| span.paint).collect();
    assert!(
        roles.contains(&Some(Paint::Role(Role::Type))),
        "`template` and the namespace are C++'s own half"
    );
}

/// **A Go file is painted with roles of its own** — M8.10's layer. A type
/// reference is teal, a package olive and a built-in a second orange, while
/// the same captures in another language keep the table's own roles.
#[test]
fn go_takes_its_own_roles_and_no_other_language_does() {
    let text = "package auth\n\ntype Identity struct {\n\tUserID int64\n}\n\nvar nothing = nil\n";
    let has = |line: usize, word: &str, role: Role| {
        let runs = painted(text, "go", line);
        assert!(
            runs.iter()
                .any(|(text, paint)| text.trim() == word && *paint == Some(Paint::Role(role))),
            "{word} on line {line}: {runs:?}"
        );
    };
    // The package name is captured by a pattern of ours: tree-sitter-go
    // does not name one at all.
    has(0, "auth", Role::Namespace);
    has(2, "Identity", Role::TypeRef);
    has(3, "int64", Role::TypeRef);
    has(6, "nil", Role::Builtin);

    // Rust says `type` too, and keeps the table's row.
    let runs = painted("struct Thing { field: Other }\n", "rust", 0);
    assert!(
        runs.iter()
            .any(|(text, paint)| text.trim() == "Other" && *paint == Some(Paint::Role(Role::Type))),
        "another language keeps the table's type: {runs:?}"
    );
}
