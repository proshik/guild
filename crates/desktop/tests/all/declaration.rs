//! What a file declares, which is what its row draws — M8.13.
//!
//! A `.java` file's row does not get one icon; it draws
//! the class, the interface or the enum in it, and Kotlin's `object` and
//! `data class` beside them. Out here because the rule is the public
//! door: the tree asks it and so would anything else that wanted to say
//! what a file is.

use guild_desktop::declaration::{self, Kind};

/// Kotlin spells almost all of this inside one node.
///
/// `class_declaration` covers `interface` and `enum class` as keywords
/// and `data`, `annotation`, `abstract` and `sealed` as modifiers; only
/// `object` has a node of its own. Measured against the grammar rather
/// than read off a manual.
#[test]
fn kotlin_says_what_it_declares() {
    let of = |stem: &str, code: &str| declaration::of("kotlin", stem, code);

    assert_eq!(of("Foo", "class Foo"), Some(Kind::Class));
    assert_eq!(of("Baz", "interface Baz"), Some(Kind::Interface));
    assert_eq!(of("Qux", "object Qux"), Some(Kind::Class));
    assert_eq!(of("Kind", "enum class Kind { ONE }"), Some(Kind::Enum));
    assert_eq!(of("Ann", "annotation class Ann"), Some(Kind::Annotation));
    assert_eq!(
        of("Bar", "data class Bar(val x: Int)"),
        Some(Kind::Record),
        "a data class is a record in everything but the word"
    );
    assert_eq!(of("Abs", "abstract class Abs"), Some(Kind::Abstract));
    assert_eq!(
        of("Sealed", "sealed class Sealed"),
        Some(Kind::Abstract),
        "sealed is abstract with a closed list of heirs"
    );

    // A package header and imports come first in every real file.
    assert_eq!(
        of(
            "Bot",
            "package com.example.bot\n\nimport java.time.Instant\n\ndata class Bot(val id: Long)\n"
        ),
        Some(Kind::Record)
    );
}

/// Java gives each its own node, and `abstract` is the one modifier that
/// changes the answer.
#[test]
fn java_says_what_it_declares() {
    let of = |stem: &str, code: &str| declaration::of("java", stem, code);

    assert_eq!(of("Foo", "public class Foo {}"), Some(Kind::Class));
    assert_eq!(of("Baz", "interface Baz {}"), Some(Kind::Interface));
    assert_eq!(of("Kind", "enum Kind { ONE }"), Some(Kind::Enum));
    assert_eq!(of("Ann", "@interface Ann {}"), Some(Kind::Annotation));
    assert_eq!(of("Rec", "record Rec(int x) {}"), Some(Kind::Record));
    assert_eq!(
        of("Abs", "public abstract class Abs {}"),
        Some(Kind::Abstract)
    );
}

/// **The file's subject is the declaration named after it.** That is
/// Java's own rule, and it is what keeps a file of
/// several classes from claiming to be the first of them.
#[test]
fn a_file_is_the_declaration_that_carries_its_name() {
    let many =
        "interface Storage\n\nclass FileStorage : Storage\n\nclass MemoryStorage : Storage\n";
    assert_eq!(
        declaration::of("kotlin", "Storage", many),
        Some(Kind::Interface),
        "the name decides, and it is not the first one"
    );
    assert_eq!(
        declaration::of("kotlin", "FileStorage", many),
        Some(Kind::Class)
    );
    assert_eq!(
        declaration::of("kotlin", "Handlers", many),
        None,
        "nothing here is named after the file, and there are three"
    );
    assert_eq!(
        declaration::of("kotlin", "Anything", "class OnlyOne"),
        Some(Kind::Class),
        "one declaration is the file's subject whatever the file is called"
    );
}

/// **A grammar's answer and not a regular expression's**, which is the
/// reason this reads the file at all: `class Foo` in a comment or a
/// string is not a declaration, and a scan of the first kilobytes would
/// say it was.
#[test]
fn a_word_in_a_comment_or_a_string_declares_nothing() {
    for code in [
        "// class Ghost\n// interface Ghost\n",
        "/*\n * data class Ghost(val x: Int)\n */\n",
        "val sql = \"\"\"\nclass Ghost\n\"\"\"\n",
    ] {
        assert_eq!(declaration::of("kotlin", "Ghost", code), None, "{code}");
    }
}

/// Only the two languages drawn this way, and nothing else — a file
/// with no declaration keeps its language's own icon.
#[test]
fn everything_else_declares_nothing() {
    assert!(declaration::reads("kotlin") && declaration::reads("java"));
    for language in ["rust", "go", "python", "markdown", "text"] {
        assert!(!declaration::reads(language), "{language}");
        assert_eq!(
            declaration::of(language, "Foo", "struct Foo {}"),
            None,
            "{language}"
        );
    }
    assert_eq!(
        declaration::of("kotlin", "Helpers", "fun greet() = 1\n"),
        None,
        "a file of functions declares no type"
    );
    assert_eq!(declaration::of("kotlin", "Empty", ""), None);
}
