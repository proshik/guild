//! A project's symbols — Go to Symbol's index. M8.9, decision 11.
//!
//! **Declared by the seams commit and filled by group B** (tasks 10 and
//! 11): tree-sitter's tags for every grammar that ships a query, and a
//! ready server's `workspace/symbol` merged over them. What is here now is
//! the types that cross a branch, with their final fields — group A draws a
//! [`Row`] and opens its [`Where`].
//!
//! **Every line and column is zero-based and counted in characters**, as
//! `Place` counts them: a tag's byte column and a server's UTF-16 column
//! are converted where they arise, so no reader has to ask which it holds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use tree_sitter_tags::{TagsConfiguration, TagsContext};

use crate::fuzzy::{self, Shape};

/// How many rows a ranking hands back — the file index's screenful.
pub const LIMIT: usize = crate::file_index::LIMIT;

/// What a symbol is — a one-letter badge in a row.
///
/// `Struct` and `Enum` never come from tags (every grammar that tags them
/// calls them `class` or `type`, task 0); they arrive from a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    /// A free function.
    Function,
    /// A function of a type.
    Method,
    /// A class, or what a grammar's tags call one.
    Class,
    /// A struct — from a server.
    Struct,
    /// An interface, a trait, a protocol.
    Interface,
    /// An enum — from a server.
    Enum,
    /// A module or a namespace.
    Module,
    /// A macro.
    Macro,
    /// A constant.
    Constant,
    /// A type or a type alias.
    Type,
    /// A property or a field.
    Property,
    /// Anything else a tags query or a server names.
    Other,
}

/// Where a row came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// A grammar's tags query.
    Tags,
    /// A language server's `workspace/symbol`, which wins over a tag at
    /// the same place.
    Server,
}

/// Where a symbol is — and so which of decision 17's doors opens it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Where {
    /// In the project: `go_to_place`.
    Project {
        /// Relative to the project.
        path: PathBuf,
        /// Zero-based.
        line: u32,
        /// Zero-based, in characters.
        column: u32,
    },
    /// Under a dependency's root, by its index into `Libraries::of`:
    /// `go_to_library`.
    Library {
        /// The index into `Libraries::of(project)`.
        under: usize,
        /// Relative to that root.
        path: PathBuf,
        /// Zero-based.
        line: u32,
        /// Zero-based, in characters.
        column: u32,
    },
    /// Outside every root — a crate no root holds yet, the standard
    /// library: `land_outside`.
    Outside {
        /// Absolute.
        path: PathBuf,
        /// Zero-based.
        line: u32,
        /// Zero-based, in characters.
        column: u32,
    },
}

/// A symbol as a ranked list hands it over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Its name.
    pub name: String,
    /// What it is.
    pub kind: SymbolKind,
    /// What it is inside — a type, a module — when anything is.
    pub container: Option<String>,
    /// Where it is.
    pub at: Where,
    /// The matched letters, as `char` indices into `name`.
    pub marks: Vec<u32>,
    /// Where the row came from.
    pub from: Source,
}

/// A symbol as a file's tags give it, before it is placed in a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Its name.
    pub name: String,
    /// What it is.
    pub kind: SymbolKind,
    /// What it is inside, when the source says — a tag never does.
    pub container: Option<String>,
    /// Zero-based.
    pub line: u32,
    /// Zero-based, **in characters** from the start of the line.
    pub column: u32,
}

/// What a tags query's `@definition.<name>` means as a kind — task 0's
/// table, read off the fifteen crates' queries.
fn kind_of(syntax_type: &str) -> SymbolKind {
    match syntax_type {
        "function" => SymbolKind::Function,
        "method" => SymbolKind::Method,
        "class" => SymbolKind::Class,
        "interface" => SymbolKind::Interface,
        "module" => SymbolKind::Module,
        "macro" => SymbolKind::Macro,
        "constant" => SymbolKind::Constant,
        "type" => SymbolKind::Type,
        // php calls a property a field.
        "property" | "field" => SymbolKind::Property,
        _ => SymbolKind::Other,
    }
}

/// A grammar's tags compiled once, as `syntax::compiled` keeps its
/// queries — `None` for a grammar with no tags, or with tags that will not
/// compile (which is logged, once).
pub fn configuration(grammar: &str) -> Option<Arc<TagsConfiguration>> {
    type Compiled = Mutex<HashMap<&'static str, Option<Arc<TagsConfiguration>>>>;
    static CACHE: LazyLock<Compiled> = LazyLock::new(|| Mutex::new(HashMap::new()));
    let found = crate::grammars::of(grammar)?;
    let mut cache = CACHE.lock().ok()?;
    if let Some(compiled) = cache.get(found.name) {
        return compiled.clone();
    }
    let compiled = if found.tags.is_empty() {
        None
    } else {
        compile(&(found.language)(), &found.tags.join("\n"))
            .inspect_err(|err| {
                tracing::warn!(grammar = found.name, %err, "the tags query did not compile");
            })
            .ok()
            .map(Arc::new)
    };
    cache.insert(found.name, compiled.clone());
    compiled
}

/// A tags configuration from a query **as published**, with every pattern
/// whose capture `tree-sitter-tags` refuses taken out.
///
/// Tags accepts `@definition.*`, `@reference.*`, `@doc`, `@name`, `@ignore`
/// and `@local.*`, and nothing else. `tree-sitter-c-sharp` 0.23.5's
/// `tags.scm` ends with `(namespace_declaration name: (identifier) @name)
/// @module`, and the whole query was refused with
/// `InvalidCapture("module")` — task 0. The pattern goes rather than the
/// query, the way `highlight::RENAMED` rewrites a highlights query as it is
/// registered rather than asking a crate to change.
fn compile(
    language: &tree_sitter::Language,
    source: &str,
) -> Result<TagsConfiguration, tree_sitter_tags::Error> {
    let mut source = source.to_owned();
    loop {
        match TagsConfiguration::new(language.clone(), &source, "") {
            Err(tree_sitter_tags::Error::InvalidCapture(capture)) => {
                let Some(fewer) = without_capture(language, &source, &capture) else {
                    return Err(tree_sitter_tags::Error::InvalidCapture(capture));
                };
                source = fewer;
            }
            answer => return answer,
        }
    }
}

/// `source` with every pattern that captures `@capture` taken out, or
/// `None` when none does — which would loop.
fn without_capture(
    language: &tree_sitter::Language,
    source: &str,
    capture: &str,
) -> Option<String> {
    let query = tree_sitter::Query::new(language, source).ok()?;
    let starts: Vec<usize> = (0..query.pattern_count())
        .map(|pattern| query.start_byte_for_pattern(pattern))
        .collect();
    let named = format!("@{capture}");
    let mut kept = String::with_capacity(source.len());
    kept.push_str(source.get(..starts.first().copied().unwrap_or(source.len()))?);
    let mut dropped = false;
    for (at, start) in starts.iter().enumerate() {
        let end = starts.get(at + 1).copied().unwrap_or(source.len());
        let pattern = source.get(*start..end)?;
        let captures_it = pattern.match_indices(&named).any(|(found, _)| {
            !pattern[found + named.len()..]
                .starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == '.' || c == '-')
        });
        if captures_it {
            dropped = true;
        } else {
            kept.push_str(pattern);
        }
    }
    dropped.then_some(kept)
}

thread_local! {
    /// A parser and a cursor per thread: the index's thread tags file
    /// after file with one.
    static CONTEXT: std::cell::RefCell<TagsContext> = std::cell::RefCell::new(TagsContext::new());
}

/// The definitions a text's tags give, in the order the query finds them.
///
/// **Nothing for a file over `file_tab::HIGHLIGHT_LIMIT`**, the size past
/// which a tab stops colouring — a generated parser is not where a person
/// looks for a symbol, and task 0 measured the slowest registry file at
/// 372 ms in a debug build. References are left out: `impl Draw for
/// Point` names both, and neither is defined there.
pub fn of_text(text: &str, grammar: &str) -> Vec<Symbol> {
    if text.len() as u64 > crate::file_tab::HIGHLIGHT_LIMIT {
        return Vec::new();
    }
    let Some(config) = configuration(grammar) else {
        return Vec::new();
    };
    CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        let Ok((tags, _)) = context.generate_tags(&config, text.as_bytes(), None) else {
            return Vec::new();
        };
        tags.flatten()
            .filter(|tag| tag.is_definition)
            .filter_map(|tag| {
                let name = text.get(tag.name_range.clone())?.to_owned();
                // `span.start` is the name's, and its column is in bytes.
                let line_start = tag.name_range.start.checked_sub(tag.span.start.column)?;
                let column = text.get(line_start..tag.name_range.start)?.chars().count();
                Some(Symbol {
                    name,
                    kind: kind_of(config.syntax_type_name(tag.syntax_type_id)),
                    container: None,
                    line: u32::try_from(tag.span.start.row).ok()?,
                    column: u32::try_from(column).ok()?,
                })
            })
            .collect()
    })
}

/// The symbols `query` finds **by their names**, best first, at most
/// `limit` of them — a path that contains the query is not a symbol that
/// matches it. Nothing typed asks nothing: a project's symbols have no
/// order that answers *nothing*, as recency answers it for files.
pub fn rank(query: &str, symbols: &[(Where, Symbol)], limit: usize) -> Vec<Row> {
    ranked_from(query, symbols, limit, Source::Tags)
}

/// [`rank`], for rows from either source.
fn ranked_from(query: &str, symbols: &[(Where, Symbol)], limit: usize, from: Source) -> Vec<Row> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let mut found = fuzzy::rank(query, symbols, |(_, symbol)| &symbol.name, Shape::Name);
    found.sort_by(|(a_at, a), (b_at, b)| {
        b.score
            .cmp(&a.score)
            .then(letters(&symbols[*a_at].1.name).cmp(&letters(&symbols[*b_at].1.name)))
    });
    found
        .into_iter()
        .take(limit)
        .map(|(at, found)| {
            let (place, symbol) = &symbols[at];
            Row {
                name: symbol.name.clone(),
                kind: symbol.kind,
                container: symbol.container.clone(),
                at: place.clone(),
                marks: found.marks,
                from,
            }
        })
        .collect()
}

/// A name's length as it is read — in characters, not bytes, which is
/// what breaks a tie between two names that scored the same.
fn letters(name: &str) -> usize {
    name.chars().count()
}

/// What an LSP `SymbolKind` number means here. Rust-analyzer says 23 for a
/// struct and 10 for an enum, the only places those two kinds come from.
fn kind_of_server(kind: u64) -> SymbolKind {
    match kind {
        2..=4 => SymbolKind::Module,
        5 => SymbolKind::Class,
        6 | 9 => SymbolKind::Method,
        7 | 8 | 22 => SymbolKind::Property,
        10 => SymbolKind::Enum,
        11 => SymbolKind::Interface,
        12 => SymbolKind::Function,
        14 => SymbolKind::Constant,
        23 => SymbolKind::Struct,
        26 => SymbolKind::Type,
        _ => SymbolKind::Other,
    }
}

/// The symbols a `workspace/symbol` answer names, each with its **absolute
/// path, its line and its UTF-16 column as the protocol counts them** —
/// converted by [`placed`] once the file has been read, and not before.
///
/// Both shapes: `SymbolInformation[]`, which rust-analyzer 1.98.0 answered
/// every time in task 0, and `WorkspaceSymbol[]`, whose location may carry
/// only a URI and is then placed at the file's top. A URI that is not a
/// file on this disk is no place, and an empty `containerName` is none.
/// The `Symbol`'s own line and column are the protocol's too, until placed.
pub fn from_server(answer: serde_json::Value) -> Vec<(PathBuf, u32, u32, Symbol)> {
    let serde_json::Value::Array(items) = answer else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_owned();
            let location = item.get("location")?;
            let path = crate::lsp::path_of(location.get("uri")?.as_str()?)?;
            let start = location.get("range").and_then(|range| range.get("start"));
            let number = |key: &str| {
                start
                    .and_then(|start| start.get(key))
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(0)
            };
            let (line, utf16) = (number("line"), number("character"));
            let container = item
                .get("containerName")
                .and_then(serde_json::Value::as_str)
                .filter(|container| !container.is_empty())
                .map(str::to_owned);
            let kind = kind_of_server(
                item.get("kind")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            );
            Some((
                path,
                line,
                utf16,
                Symbol {
                    name,
                    kind,
                    container,
                    line,
                    column: utf16,
                },
            ))
        })
        .collect()
}

/// A UTF-16 column on `line` of `text`, in characters — through
/// `lsp::providers::caret_for`, the one place the two counts meet. An emoji
/// is two units and one character.
pub fn placed(text: &str, line: u32, utf16: u32) -> u32 {
    crate::lsp::providers::caret_for(
        text,
        lsp_types::Position {
            line,
            character: utf16,
        },
    )
    .1
}

/// Where an absolute path is: in the project, under a dependency's root by
/// its index into `libraries`, or outside every root — decision 17's three
/// doors. Components are compared, so `/work/guild-other` is not in
/// `/work/guild`.
pub fn where_of(
    project: &Path,
    libraries: &[PathBuf],
    path: &Path,
    line: u32,
    column: u32,
) -> Where {
    if let Ok(relative) = path.strip_prefix(project) {
        return Where::Project {
            path: relative.to_path_buf(),
            line,
            column,
        };
    }
    for (under, root) in libraries.iter().enumerate() {
        if let Ok(relative) = path.strip_prefix(root) {
            return Where::Library {
                under,
                path: relative.to_path_buf(),
                line,
                column,
            };
        }
    }
    Where::Outside {
        path: path.to_path_buf(),
        line,
        column,
    }
}

/// A server's rows over the tags': **a server row wins over a tag at the
/// same place** — it knows a struct from a class and what it is inside —
/// and takes that tag's place in the list; the server's other rows follow
/// the tags. A ready server answering `[]` in its first minute (task 0:
/// 51 s cold) is no rows yet, and the tags stand.
///
/// **The same place is the same file and line and the same name**, or the
/// very same column: task 0 read rust-analyzer's ranges starting at the
/// name, but a server whose range is the whole item starts it at `pub`,
/// and a column compared exactly would show that symbol twice. Such a row
/// lands where the tag found the name.
pub fn merge(tags: Vec<Row>, server: Vec<Row>) -> Vec<Row> {
    let mut server: Vec<Option<Row>> = server.into_iter().map(Some).collect();
    let mut merged: Vec<Row> = tags
        .into_iter()
        .map(|tag| {
            let exact = server
                .iter()
                .position(|row| row.as_ref().is_some_and(|row| row.at == tag.at));
            let on_its_line = || {
                server.iter().position(|row| {
                    row.as_ref()
                        .is_some_and(|row| row.name == tag.name && same_line(&row.at, &tag.at))
                })
            };
            match exact.or_else(on_its_line).and_then(|at| server[at].take()) {
                Some(mut row) => {
                    row.at = tag.at;
                    row
                }
                None => tag,
            }
        })
        .collect();
    merged.extend(server.into_iter().flatten());
    merged
}

/// Whether two places are on the same line of the same file.
fn same_line(a: &Where, b: &Where) -> bool {
    match (a, b) {
        (
            Where::Project { path, line, .. },
            Where::Project {
                path: other,
                line: other_line,
                ..
            },
        )
        | (
            Where::Outside { path, line, .. },
            Where::Outside {
                path: other,
                line: other_line,
                ..
            },
        ) => path == other && line == other_line,
        (
            Where::Library {
                under, path, line, ..
            },
            Where::Library {
                under: other_under,
                path: other,
                line: other_line,
                ..
            },
        ) => under == other_under && path == other && line == other_line,
        _ => false,
    }
}

/// Rows ranked again for `query` by their names, best first — a merged
/// list is two rankings side by side, and a server's own order is its
/// fuzzy match and not ours. The marks are found again with the score.
pub fn rerank(query: &str, rows: Vec<Row>, limit: usize) -> Vec<Row> {
    let found = fuzzy::rank(query, &rows, |row| &row.name, Shape::Name);
    let mut ranked: Vec<(u32, Row)> = found
        .into_iter()
        .map(|(at, found)| {
            let mut row = rows[at].clone();
            row.marks = found.marks;
            (found.score, row)
        })
        .collect();
    ranked.sort_by(|(a_score, a), (b_score, b)| {
        b_score
            .cmp(a_score)
            .then(letters(&a.name).cmp(&letters(&b.name)))
    });
    ranked.into_iter().take(limit).map(|(_, row)| row).collect()
}

/// A server's symbols ranked for `query` as rows of [`Source::Server`].
pub fn rank_server(query: &str, symbols: &[(Where, Symbol)], limit: usize) -> Vec<Row> {
    ranked_from(query, symbols, limit, Source::Server)
}
