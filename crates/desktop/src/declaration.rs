//! What a file **declares**, so that its row can say so — M8.13.
//!
//! A `.java` file does not get one icon: its row draws what is in it, a
//! class, an interface, an enum, and Kotlin's `object` and `data class`
//! beside them. The person asked for that, and for it to be read
//! rather than guessed — so this is the grammar's answer, not a regular
//! expression's, and `class Foo` inside a comment or a string cannot be
//! mistaken for the file's subject.
//!
//! **Pure, and it is all that is pure about this.** The reading and the
//! caching are `root::declarations`; here there is a text, a grammar and
//! an answer.

use tree_sitter::Node;

/// What a file's own declaration is.
///
/// Six, because that is how many icons the IntelliJ platform publishes
/// for them — `assets/nodes/PROVENANCE` says which stands in for what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `class Foo`, and Kotlin's `object Foo`.
    Class,
    /// `abstract class Foo`, and Kotlin's `sealed class Foo`.
    Abstract,
    /// `interface Foo`.
    Interface,
    /// `enum Kind` and `enum class Kind`.
    Enum,
    /// Java's `@interface Ann` and Kotlin's `annotation class Ann`.
    Annotation,
    /// Java's `record Rec`, and Kotlin's `data class Rec`.
    Record,
}

/// Whether a language has declarations this module can read at all.
///
/// Two, and they are the two the declaration icons were drawn for and the two
/// the person was looking at. A third is a grammar's node names and a row of
/// tests, not a rule to generalise from: every language spells this its own
/// way and most do not spell it at all.
pub fn reads(language: &str) -> bool {
    matches!(language, "kotlin" | "java")
}

/// What a file declares, or `None` when nothing in it is the file's
/// subject.
///
/// **The file's subject is the declaration named after it**, which is
/// Java's own rule; where nothing matches the name,
/// a file with exactly one top-level declaration is that declaration, and
/// a file with several is not any of them. So a `Handlers.kt` holding
/// three unrelated classes keeps the plain Kotlin icon rather than
/// claiming to be the first of them.
pub fn of(language: &str, stem: &str, text: &str) -> Option<Kind> {
    if !reads(language) {
        return None;
    }
    let grammar = crate::grammars::of(language)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&(grammar.language)()).ok()?;
    let tree = parser.parse(text, None)?;

    let root = tree.root_node();
    let mut cursor = root.walk();
    let found: Vec<(Option<&str>, Kind)> = root
        .children(&mut cursor)
        .filter_map(|node| kind_of(language, node, text).map(|kind| (name_of(node, text), kind)))
        .collect();

    found
        .iter()
        .find(|(name, _)| *name == Some(stem))
        .or(match found.as_slice() {
            [only] => Some(only),
            _ => None,
        })
        .map(|(_, kind)| *kind)
}

/// The name a declaration gives itself, for the comparison with the
/// file's own. Kotlin's is a `type_identifier` and Java's an
/// `identifier`; both are the first of their kind under the node.
fn name_of<'a>(node: Node<'_>, text: &'a str) -> Option<&'a str> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| matches!(child.kind(), "type_identifier" | "identifier"))
        .and_then(|child| text.get(child.byte_range()))
}

/// What one top-level node declares, by the grammar's own node names.
///
/// Measured against both grammars rather than read off a manual: Kotlin
/// puts `interface` and `enum` inside `class_declaration` as keywords and
/// `data`, `annotation`, `abstract` and `sealed` inside a `modifiers`
/// child, while Java gives each its own node.
fn kind_of(language: &str, node: Node<'_>, text: &str) -> Option<Kind> {
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
    let has = |word: &str| children.iter().any(|child| child.kind() == word);
    let modifiers = children
        .iter()
        .find(|child| child.kind() == "modifiers")
        .and_then(|child| text.get(child.byte_range()))
        .unwrap_or_default();
    let modifier = |word: &str| modifiers.split_whitespace().any(|part| part == word);

    match (language, node.kind()) {
        ("kotlin", "object_declaration") => Some(Kind::Class),
        ("kotlin", "class_declaration") => Some(if has("interface") {
            Kind::Interface
        } else if has("enum") {
            Kind::Enum
        } else if modifier("annotation") {
            Kind::Annotation
        } else if modifier("data") {
            Kind::Record
        } else if modifier("abstract") || modifier("sealed") {
            Kind::Abstract
        } else {
            Kind::Class
        }),
        ("java", "interface_declaration") => Some(Kind::Interface),
        ("java", "enum_declaration") => Some(Kind::Enum),
        ("java", "annotation_type_declaration") => Some(Kind::Annotation),
        ("java", "record_declaration") => Some(Kind::Record),
        ("java", "class_declaration") => Some(if modifier("abstract") {
            Kind::Abstract
        } else {
            Kind::Class
        }),
        _ => None,
    }
}
