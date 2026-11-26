//! What a comment is written with, by grammar: **one table**, keyed by the
//! names `grammars.rs` registers, so a language reached through another's
//! grammar — `jsonc` through `json`, `ini` through `toml` — comments the
//! way its grammar parses.
//!
//! A test walks `grammars::all()` against this table and [`COMMENTLESS`]:
//! a grammar added tomorrow is in exactly one of the two, or the build says
//! so.

/// A language's comment tokens. Either may be absent; a row with neither
/// belongs in [`COMMENTLESS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comment {
    /// What starts a comment running to the end of the line.
    pub line: Option<&'static str>,
    /// What opens and closes a comment that can sit inside a line.
    pub block: Option<(&'static str, &'static str)>,
    /// The kinds of the grammar's syntax nodes a block comment is, as the
    /// grammar spells them — XML's is `Comment`, SQL's `marginalia`, so a
    /// guess at the word would miss them. Empty when the tree has none:
    /// Markdown's `<!-- -->` is an `html_block`.
    pub nodes: &'static [&'static str],
}

const fn c_like(nodes: &'static [&'static str]) -> Comment {
    Comment {
        line: Some("//"),
        block: Some(("/*", "*/")),
        nodes,
    }
}
const HASH: Comment = Comment {
    line: Some("#"),
    block: None,
    nodes: &[],
};
const fn markup(nodes: &'static [&'static str]) -> Comment {
    Comment {
        line: None,
        block: Some(("<!--", "-->")),
        nodes,
    }
}
const COMMENT: &[&str] = &["comment"];

/// The tokens, by grammar name.
///
/// `json`'s are `jsonc`'s — tsconfig, `.eslintrc` and JSON5 all take them,
/// and tree-sitter-json parses them. Ruby's `=begin`/`=end` and Python's
/// docstrings are not inline block comments, so those two have only their
/// line comment.
pub const COMMENTS: &[(&str, Comment)] = &[
    ("rust", c_like(&["block_comment"])),
    ("toml", HASH),
    ("json", c_like(COMMENT)),
    ("yaml", HASH),
    ("bash", HASH),
    ("go", c_like(COMMENT)),
    ("ruby", HASH),
    ("elixir", HASH),
    ("java", c_like(&["block_comment"])),
    ("javascript", c_like(COMMENT)),
    ("typescript", c_like(COMMENT)),
    ("tsx", c_like(COMMENT)),
    ("make", HASH),
    ("just", HASH),
    ("proto", c_like(COMMENT)),
    ("properties", HASH),
    ("kotlin", c_like(&["multiline_comment"])),
    ("dockerfile", HASH),
    ("html", markup(COMMENT)),
    (
        "css",
        Comment {
            line: None,
            block: Some(("/*", "*/")),
            nodes: COMMENT,
        },
    ),
    ("scss", c_like(COMMENT)),
    ("xml", markup(&["Comment"])),
    (
        "sql",
        Comment {
            line: Some("--"),
            block: Some(("/*", "*/")),
            nodes: &["marginalia"],
        },
    ),
    ("python", HASH),
    ("c", c_like(COMMENT)),
    ("cpp", c_like(COMMENT)),
    ("csharp", c_like(COMMENT)),
    ("php", c_like(COMMENT)),
    ("swift", c_like(&["multiline_comment"])),
    (
        "lua",
        Comment {
            line: Some("--"),
            block: Some(("--[[", "]]")),
            nodes: COMMENT,
        },
    ),
    ("svelte", markup(COMMENT)),
    (
        "nix",
        Comment {
            line: Some("#"),
            block: Some(("/*", "*/")),
            nodes: COMMENT,
        },
    ),
    ("graphql", HASH),
    (
        "hcl",
        Comment {
            line: Some("#"),
            block: Some(("/*", "*/")),
            nodes: COMMENT,
        },
    ),
    ("markdown", markup(&[])),
];

/// The registered grammars that have no comment: plain text, and
/// Markdown's inline grammar, which only ever parses inside a Markdown file
/// and is never a file's own.
pub const COMMENTLESS: &[&str] = &[crate::highlight::PLAIN, "markdown_inline"];

/// The tokens a grammar comments with; none for a grammar without them.
pub fn for_grammar(grammar: &str) -> Comment {
    COMMENTS.iter().find(|(name, _)| *name == grammar).map_or(
        Comment {
            line: None,
            block: None,
            nodes: &[],
        },
        |(_, comment)| *comment,
    )
}
