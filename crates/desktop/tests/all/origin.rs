//! **Where a file tab's path is read, `Origin` is read with it.**
//!
//! `Body::File` carries a path *and* the root it is relative to: the
//! project's, or one of its dependencies'. M8.8.3 added the second and
//! guarded every gesture it owned — `cmd-d`, the library tree's menu,
//! `store_tabs`, `file_in_front` — each with a comment naming the same
//! failure: a crate's `src/lib.rs` read as this repository's, the same
//! name and a different file.
//!
//! M8.8.2 was written in a worktree where `Origin` did not exist yet,
//! and it added six more readers of that path. The hand merge that
//! brought the two together compiled, passed, and left every one of them
//! blind — which is the failure a milestone built in six branches has
//! that a milestone built in one does not.
//!
//! So this is a guard over the source text rather than over a value: the
//! rule is *say what you mean about the origin*, and the compiler has no
//! opinion about a `..`. The allow-list below is the whole of the other
//! answer — three places where the origin genuinely does not matter —
//! and each says why. A fourth entry is a decision, not a fix.

use std::path::Path;

/// A pattern that binds `path` out of a `Body::File`, and where.
struct Reader {
    file: String,
    line: usize,
    pattern: String,
}

/// The patterns that take a path out of a file tab, whatever they then
/// do with it.
///
/// Binding `path` is the discriminator, not matching `Body::File`: the
/// thirty-odd places that want the editor entity or the tab itself are
/// right to ignore where the file came from, and the ones that take the
/// **path** are the ones that will join it to something.
fn readers() -> Vec<Reader> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the crate's own source") {
            let entry = entry.expect("a directory entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a source file");
            for (at, _) in text.match_indices("Body::File") {
                let rest = &text[at + "Body::File".len()..];
                let Some(open) = rest.find(|c: char| !c.is_whitespace()) else {
                    continue;
                };
                if rest.as_bytes()[open] != b'{' {
                    continue;
                }
                let Some(pattern) = braced(&rest[open..]) else {
                    continue;
                };
                if !binds_path(pattern) {
                    continue;
                }
                found.push(Reader {
                    file: path
                        .strip_prefix(&src)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                    line: text[..at].matches('\n').count() + 1,
                    pattern: pattern.split_whitespace().collect::<Vec<_>>().join(" "),
                });
            }
        }
    }
    found
}

/// From an opening brace to the one that closes it.
fn braced(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (at, byte) in text.bytes().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[..=at]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the pattern names `path` as a whole word — `path`, `path:`
/// and `path,` all count; `paths` does not.
fn binds_path(pattern: &str) -> bool {
    pattern
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|word| word == "path")
}

/// The readers that may leave the origin out, and the reason each
/// may.
///
/// **Another line here is a decision.** Everything else that takes a
/// path out of a file tab joins it to a root, and the wrong root is a
/// file with the same name and different contents — no error, no log
/// line, nothing to notice.
const ALLOWED: &[(&str, &str)] = &[
    (
        "root/tabs.rs",
        "the arm above it returns `None` for an external tab, so this one \
         is reached only for the project's own — ordering is the guard, \
         and it is spelled out in a comment there",
    ),
    (
        "view/tabs.rs",
        "the strip draws a tab's name and its icon from the file name, \
         which is the same word wherever the file came from",
    ),
];

/// Every reader of a file tab's path names its origin, or is one of the
/// readers that need not.
#[test]
fn every_reader_of_a_file_tabs_path_says_what_it_means_about_origin() {
    let mut blind = Vec::new();
    for reader in readers() {
        if reader.pattern.contains("origin") {
            continue;
        }
        if ALLOWED.iter().any(|(file, _)| *file == reader.file) {
            continue;
        }
        blind.push(format!(
            "{}:{} — {}",
            reader.file, reader.line, reader.pattern
        ));
    }
    assert!(
        blind.is_empty(),
        "these read a file tab's path without saying which root it is \
         relative to; join one to the project's root and you have named a \
         different file with the same name:\n  {}",
        blind.join("\n  ")
    );
}

/// The allow-list has a subject: a name on it that no longer matches any
/// reader is a line nobody will notice has gone stale.
///
/// `just boundaries` was caught by exactly this when the crate it named
/// was deleted — it went on reporting success for weeks.
#[test]
fn every_allowed_file_still_has_a_reader_in_it() {
    let readers = readers();
    for (file, _) in ALLOWED {
        assert!(
            readers.iter().any(|reader| reader.file == *file),
            "{file} is allowed to leave the origin out and no longer reads a path at all"
        );
    }
}

/// And the walk itself finds something, or both tests above pass over an
/// empty list and say nothing at all.
#[test]
fn the_walk_finds_the_readers_it_is_about() {
    let readers = readers();
    assert!(
        readers.len() >= 8,
        "a file tab's path is read in more places than this: {}",
        readers.len()
    );
    assert!(
        readers
            .iter()
            .any(|reader| reader.file == "root/history.rs"),
        "the module the rule was written for is walked"
    );
}
