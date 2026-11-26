//! Native process metadata and grammars compiled with the existing C toolchain.
//!
//! Two of the grammars M8.1.1 needs publish crates pinned to
//! `tree-sitter` 0.20 or 0.21, and cargo refuses to resolve two versions
//! of a crate that declares `links = "tree-sitter"`. The grammar itself
//! is version-agnostic — it is the crate's manifest that is stale — so
//! the parser is vendored by `scripts/vendor-grammars.py` and built here.

use std::path::Path;

/// Every vendored grammar: the directory under `vendor/`, and the name
/// the static library takes.
const VENDORED: &[&str] = &["tree-sitter-dockerfile", "tree-sitter-kotlin"];

fn main() {
    println!("cargo:rerun-if-changed=src/terminal/process_generation.c");
    cc::Build::new()
        .file("src/terminal/process_generation.c")
        .compile("guild_process_generation");
    for grammar in VENDORED {
        let source = Path::new("vendor").join(grammar).join("src");
        println!("cargo:rerun-if-changed={}", source.display());
        let mut build = cc::Build::new();
        build.include(&source).file(source.join("parser.c"));
        let scanner = source.join("scanner.c");
        if scanner.is_file() {
            build.file(scanner);
        }
        // A generated parser warns copiously and there is nothing to fix
        // in it: it is regenerated, never edited.
        build.warnings(false).compile(grammar);
    }
}
