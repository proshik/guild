//! The grammars the window parses with, and nothing about who is asking.
//!
//! One table, two readers: [`crate::editor`] registers them with the
//! editor component, and [`crate::syntax`] parses with them for the
//! surfaces we draw ourselves. Two tables would be two chances for the
//! same file to look different in a tab and in a diff.

/// The grammars our own `build.rs` compiled, because their crates pin an
/// incompatible `tree-sitter`. Vendored by `scripts/vendor-grammars.py`.
mod vendored {
    use tree_sitter_language::LanguageFn;

    unsafe extern "C" {
        fn tree_sitter_dockerfile() -> *const ();
        fn tree_sitter_kotlin() -> *const ();
    }

    /// The Dockerfile grammar.
    pub const DOCKERFILE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_dockerfile) };

    /// Its highlights query, vendored beside the parser.
    pub const DOCKERFILE_HIGHLIGHTS: &str =
        include_str!("../vendor/tree-sitter-dockerfile/highlights.scm");

    /// The Kotlin grammar.
    pub const KOTLIN: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_kotlin) };

    /// Its highlights query, vendored beside the parser.
    pub const KOTLIN_HIGHLIGHTS: &str = include_str!("../vendor/tree-sitter-kotlin/highlights.scm");
}

/// The queries of grammars whose crate ships a parser with no way to
/// reach the query from Rust. Vendored by `scripts/vendor-grammars.py`.
mod queries {
    /// proto's, which its own `lib.rs` has commented out.
    pub const PROTO: &str = include_str!("../assets/queries/proto/highlights.scm");

    /// graphql's, from nvim-treesitter: its package has no `queries/`
    /// directory at all.
    pub const GRAPHQL: &str = include_str!("../assets/queries/graphql/highlights.scm");

    /// hcl's, from nvim-treesitter for the same reason.
    pub const HCL: &str = include_str!("../assets/queries/hcl/highlights.scm");
}

/// A grammar the window parses with: what the registry calls it, its
/// parser, and its queries.
///
/// A `fn` rather than a value because `tree_sitter::Language` is built at
/// run time, and a table rather than a column of calls because this is
/// the point a grammar from outside would one day enter.
pub struct Grammar {
    /// The registry's name for it, and what a language row's `grammar`
    /// field says.
    pub name: &'static str,
    /// Its parser.
    pub language: fn() -> tree_sitter::Language,
    /// Its highlights query, **before** the captures are renamed, and in
    /// **parts** because a grammar that extends another is normal.
    ///
    /// `tree-sitter-typescript` publishes thirty-five lines — the types,
    /// the parameters and the eleven keywords TypeScript adds — and the
    /// two hundred it inherits are published by `tree-sitter-javascript`
    /// under a different crate. Nothing in either says so, and
    /// `Query::new` answers `Ok` for the half on its own, so a file
    /// drawn from one part looks like a file drawn from a grammar that
    /// simply has little to say. The parts are joined in the order they
    /// are written, and the later a pattern is the more it wins — which
    /// is the same rule an overlap already follows, so the extending
    /// half goes last.
    pub highlights: &'static [&'static str],
    /// Its injections query, where we use one.
    pub injections: &'static str,
    /// The grammars it injects. The component resolves an injection only
    /// against names declared here.
    pub injects: &'static [&'static str],
    /// Its tags query — what Go to Symbol indexes — in **parts**, as
    /// `highlights` is, and empty where the crate ships none a Rust
    /// program can reach. M8.9, decision 11.
    ///
    /// **TypeScript and TSX carry JavaScript's before their own**, the
    /// trap *a grammar that extends another publishes half a query* met a
    /// second time: TypeScript's `tags.scm` alone finds `interface Person`
    /// and not `function greet()`. **C++ does not carry C's**: its
    /// `tags.scm` already repeats C's five patterns, and prepending them
    /// changes no tag (task 0). C#'s is registered as published, and the
    /// one pattern `tree-sitter-tags` refuses is taken out where the
    /// configuration is built — `symbols::configuration`.
    pub tags: &'static [&'static str],
}

impl Grammar {
    /// Its highlights query, whole: the parts one after another, with a
    /// line break between them so that a part ending in a comment cannot
    /// swallow the first pattern of the next.
    pub fn highlights(&self) -> String {
        self.highlights.join("\n")
    }
}

/// The grammars, and `text` for everything no language claims.
///
/// `text` must exist: the component falls back to it when a grammar is
/// not registered, and *unwraps* the fallback. Its own plain-text config
/// is a grammar with an empty query, and so is this one.
pub fn all() -> Vec<Grammar> {
    vec![
        Grammar {
            name: crate::highlight::PLAIN,
            language: || tree_sitter_json::LANGUAGE.into(),
            highlights: &[],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "rust",
            language: || tree_sitter_rust::LANGUAGE.into(),
            highlights: &[tree_sitter_rust::HIGHLIGHTS_QUERY],
            injections: tree_sitter_rust::INJECTIONS_QUERY,
            injects: &[],
            tags: &[tree_sitter_rust::TAGS_QUERY],
        },
        Grammar {
            name: "toml",
            language: || tree_sitter_toml_ng::LANGUAGE.into(),
            highlights: &[tree_sitter_toml_ng::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "json",
            language: || tree_sitter_json::LANGUAGE.into(),
            highlights: &[tree_sitter_json::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "yaml",
            language: || tree_sitter_yaml::LANGUAGE.into(),
            highlights: &[tree_sitter_yaml::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "bash",
            language: || tree_sitter_bash::LANGUAGE.into(),
            highlights: &[tree_sitter_bash::HIGHLIGHT_QUERY],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "go",
            language: || tree_sitter_go::LANGUAGE.into(),
            highlights: &[tree_sitter_go::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[tree_sitter_go::TAGS_QUERY],
        },
        Grammar {
            name: "ruby",
            language: || tree_sitter_ruby::LANGUAGE.into(),
            highlights: &[tree_sitter_ruby::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[tree_sitter_ruby::TAGS_QUERY],
        },
        Grammar {
            name: "elixir",
            language: || tree_sitter_elixir::LANGUAGE.into(),
            highlights: &[tree_sitter_elixir::HIGHLIGHTS_QUERY],
            injections: tree_sitter_elixir::INJECTIONS_QUERY,
            injects: &[],
            tags: &[tree_sitter_elixir::TAGS_QUERY],
        },
        Grammar {
            name: "java",
            language: || tree_sitter_java::LANGUAGE.into(),
            highlights: &[tree_sitter_java::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[tree_sitter_java::TAGS_QUERY],
        },
        Grammar {
            name: "javascript",
            language: || tree_sitter_javascript::LANGUAGE.into(),
            highlights: &[tree_sitter_javascript::HIGHLIGHT_QUERY],
            injections: tree_sitter_javascript::INJECTIONS_QUERY,
            injects: &[],
            tags: &[tree_sitter_javascript::TAGS_QUERY],
        },
        Grammar {
            name: "typescript",
            language: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            highlights: &[
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ],
            injections: "",
            injects: &[],
            tags: &[
                tree_sitter_javascript::TAGS_QUERY,
                tree_sitter_typescript::TAGS_QUERY,
            ],
        },
        Grammar {
            name: "tsx",
            language: || tree_sitter_typescript::LANGUAGE_TSX.into(),
            highlights: &[
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ],
            injections: "",
            injects: &[],
            tags: &[
                tree_sitter_javascript::TAGS_QUERY,
                tree_sitter_typescript::TAGS_QUERY,
            ],
        },
        Grammar {
            name: "make",
            language: || tree_sitter_make::LANGUAGE.into(),
            highlights: &[tree_sitter_make::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "just",
            language: || tree_sitter_just::LANGUAGE.into(),
            highlights: &[tree_sitter_just::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "proto",
            language: || tree_sitter_proto::LANGUAGE.into(),
            highlights: &[queries::PROTO],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "properties",
            language: || tree_sitter_properties::LANGUAGE.into(),
            highlights: &[tree_sitter_properties::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[tree_sitter_properties::TAGS_QUERY],
        },
        Grammar {
            name: "kotlin",
            language: || vendored::KOTLIN.into(),
            highlights: &[vendored::KOTLIN_HIGHLIGHTS],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "dockerfile",
            language: || vendored::DOCKERFILE.into(),
            highlights: &[vendored::DOCKERFILE_HIGHLIGHTS],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "html",
            language: || tree_sitter_html::LANGUAGE.into(),
            highlights: &[tree_sitter_html::HIGHLIGHTS_QUERY],
            // Thirteen lines of highlights and everything that makes an
            // HTML file look coloured in the other query: `<script>` is
            // javascript and `<style>` is css.
            injections: tree_sitter_html::INJECTIONS_QUERY,
            injects: &["javascript", "css"],
            tags: &[],
        },
        Grammar {
            name: "css",
            language: || tree_sitter_css::LANGUAGE.into(),
            highlights: &[tree_sitter_css::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "scss",
            // The one crate here still on the old free function rather
            // than a `LanguageFn` constant.
            language: tree_sitter_scss::language,
            highlights: &[tree_sitter_scss::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "xml",
            language: || tree_sitter_xml::LANGUAGE_XML.into(),
            highlights: &[tree_sitter_xml::XML_HIGHLIGHT_QUERY],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "sql",
            language: || tree_sitter_sequel::LANGUAGE.into(),
            highlights: &[tree_sitter_sequel::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "python",
            language: || tree_sitter_python::LANGUAGE.into(),
            highlights: &[tree_sitter_python::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[tree_sitter_python::TAGS_QUERY],
        },
        Grammar {
            name: "c",
            language: || tree_sitter_c::LANGUAGE.into(),
            highlights: &[tree_sitter_c::HIGHLIGHT_QUERY],
            injections: "",
            injects: &[],
            tags: &[tree_sitter_c::TAGS_QUERY],
        },
        Grammar {
            name: "cpp",
            language: || tree_sitter_cpp::LANGUAGE.into(),
            // The other half of decision 11: seventy lines of its own
            // over C's eighty-one, which it inherits and which its crate
            // does not carry.
            highlights: &[
                tree_sitter_c::HIGHLIGHT_QUERY,
                tree_sitter_cpp::HIGHLIGHT_QUERY,
            ],
            injections: "",
            injects: &[],
            tags: &[tree_sitter_cpp::TAGS_QUERY],
        },
        Grammar {
            name: "csharp",
            language: || tree_sitter_c_sharp::LANGUAGE.into(),
            highlights: &[tree_sitter_c_sharp::HIGHLIGHTS_QUERY],
            injections: "",
            injects: &[],
            tags: &[tree_sitter_c_sharp::TAGS_QUERY],
        },
        Grammar {
            name: "php",
            // Not `LANGUAGE_PHP_ONLY`: a `.php` file is HTML with
            // `<?php` islands in it, and this is the one that parses
            // both.
            language: || tree_sitter_php::LANGUAGE_PHP.into(),
            highlights: &[tree_sitter_php::HIGHLIGHTS_QUERY],
            // Its query names phpdoc and whatever a heredoc's tag says.
            // We register neither, and a grammar declares only what it
            // can really reach.
            injections: "",
            injects: &[],
            tags: &[tree_sitter_php::TAGS_QUERY],
        },
        Grammar {
            name: "swift",
            language: || tree_sitter_swift::LANGUAGE.into(),
            highlights: &[tree_sitter_swift::HIGHLIGHTS_QUERY],
            // regex and comment, and we have neither.
            injections: "",
            injects: &[],
            tags: &[tree_sitter_swift::TAGS_QUERY],
        },
        Grammar {
            name: "lua",
            language: || tree_sitter_lua::LANGUAGE.into(),
            highlights: &[tree_sitter_lua::HIGHLIGHTS_QUERY],
            // `ffi.cdef [[ … ]]` is C, and C is registered now.
            injections: tree_sitter_lua::INJECTIONS_QUERY,
            injects: &["c"],
            tags: &[tree_sitter_lua::TAGS_QUERY],
        },
        Grammar {
            name: "svelte",
            language: || tree_sitter_svelte_ng::LANGUAGE.into(),
            highlights: &[tree_sitter_svelte_ng::HIGHLIGHTS_QUERY],
            injections: tree_sitter_svelte_ng::INJECTIONS_QUERY,
            injects: &["javascript", "typescript", "scss"],
            tags: &[],
        },
        Grammar {
            name: "nix",
            language: || tree_sitter_nix::LANGUAGE.into(),
            highlights: &[tree_sitter_nix::HIGHLIGHTS_QUERY],
            // A `preBuild = '' … ''` is a shell script, and nix's query
            // knows the eight attribute names that mean one.
            injections: tree_sitter_nix::INJECTIONS_QUERY,
            injects: &["bash"],
            tags: &[],
        },
        Grammar {
            name: "graphql",
            language: || tree_sitter_graphql::LANGUAGE.into(),
            highlights: &[queries::GRAPHQL],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "hcl",
            language: || tree_sitter_hcl::LANGUAGE.into(),
            highlights: &[queries::HCL],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "markdown_inline",
            language: || tree_sitter_md::INLINE_LANGUAGE.into(),
            highlights: &[tree_sitter_md::HIGHLIGHT_QUERY_INLINE],
            injections: "",
            injects: &[],
            tags: &[],
        },
        Grammar {
            name: "markdown",
            language: || tree_sitter_md::LANGUAGE.into(),
            highlights: &[tree_sitter_md::HIGHLIGHT_QUERY_BLOCK],
            injections: tree_sitter_md::INJECTION_QUERY_BLOCK,
            injects: &["markdown_inline"],
            tags: &[],
        },
    ]
}

/// The grammar of this name, or `None` — which is a real answer for a
/// file no language claims.
pub fn of(name: &str) -> Option<Grammar> {
    all().into_iter().find(|grammar| grammar.name == name)
}
