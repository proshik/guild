//! What the tests leave behind: nothing.
//!
//! Until 2026-09-24 a test named its scratch folder under `$TMPDIR` by
//! hand and most never removed it; the person's `$TMPDIR` held 28,562
//! entries, and every `just check` added about ninety. The folders are
//! `scratch::Scratch` and `tempfile::TempDir` now, which go when the test
//! lets go of them. These two checks read the workspace's source, every
//! crate of it, so that neither shape comes back unnoticed — a leak is
//! silent by nature, and nobody opens `$TMPDIR` to look.

use std::path::{Path, PathBuf};

/// Every call of `std::env::temp_dir` in the workspace that is **not** a
/// test's folder, with the number of them and why. Production code puts
/// things in `$TMPDIR` on purpose, and a new one there is a decision to be
/// written down here rather than a line nobody reads.
const ALLOWED: &[(&str, usize, &str)] = &[
    (
        "crates/session/src/workflow/revust/environment.rs",
        2,
        "private SDK env creation and owner qualification; value-free durable lease survives observer/crash, actual terminal-job termination releases 0600 resources",
    ),
    (
        "crates/desktop/src/update/swap.rs",
        2,
        "the Update helper's folder and its log, which outlive the process that made them",
    ),
    (
        "crates/desktop/src/hooks.rs",
        1,
        "the hooks socket, under $TMPDIR because a path in the home directory does not fit sun_path",
    ),
    (
        "crates/desktop/src/root/status.rs",
        1,
        "sweeping the sockets windows that were killed left in $TMPDIR",
    ),
];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `.rs` file under `crates/`, by its path from the workspace root.
fn sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, root: &Path, into: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).expect("a readable directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(&path, root, into);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let name = path.strip_prefix(root).expect("under the root");
                let text = std::fs::read_to_string(&path).expect("a text file");
                into.push((name.to_string_lossy().into_owned(), text));
            }
        }
    }
    let root = workspace();
    let mut found = Vec::new();
    walk(&root.join("crates"), &root, &mut found);
    found.sort();
    // A walk that found nothing passes every check below; this is the
    // guard that the guard is looking somewhere.
    assert!(
        found.len() > 100 && found.iter().any(|(name, _)| name.ends_with("scratch.rs")),
        "the walk found {} files — it is not looking at this workspace",
        found.len()
    );
    found
}

/// **A test's folder is never `std::env::temp_dir` by hand.** Spelled in two
/// halves so that this file is not its own first hit.
#[test]
fn no_test_names_its_folder_under_tmpdir_by_hand() {
    let needle = concat!("temp_", "dir()");
    let mut wrong = Vec::new();
    for (name, text) in sources() {
        let count = text.matches(needle).count();
        let allowed = ALLOWED
            .iter()
            .find(|(path, _, _)| *path == name)
            .map_or(0, |(_, count, _)| *count);
        if count != allowed {
            wrong.push(format!("{name}: {count}, {allowed} allowed"));
        }
    }
    assert!(
        wrong.is_empty(),
        "`{needle}` where a test's folder should be a `Scratch` or a `tempfile::TempDir` — \
         or production code that belongs in ALLOWED, with its reason:\n{}",
        wrong.join("\n")
    );
}

/// The functions in `text` that hand out a folder: the two constructors,
/// and every `fn` whose signature line returns one.
fn makers(text: &str) -> Vec<String> {
    let mut makers = vec!["Scratch::new".to_owned(), "Scratch::absent".to_owned()];
    for line in text.lines() {
        let line = line.trim_start();
        let line = line
            .strip_prefix("pub(crate) ")
            .or_else(|| line.strip_prefix("pub "))
            .unwrap_or(line);
        let Some(rest) = line.strip_prefix("fn ") else {
            continue;
        };
        if ["-> Scratch", "-> TempDir", "-> tempfile::TempDir"]
            .iter()
            .any(|returns| line.contains(returns))
            && let Some(name) = rest.split(['(', '<']).next()
        {
            makers.push(name.to_owned());
        }
    }
    makers
}

/// Every place in `text` where a folder is made and a path taken from it
/// in the same expression, by line number.
fn temporaries(text: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    for maker in makers(text) {
        let call = format!("{maker}(");
        let mut from = 0;
        while let Some(at) = text[from..].find(&call) {
            let start = from + at;
            from = start + call.len();
            let before = &text[..start];
            // A longer name, a method of that name, or the definition.
            if before
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':')
                || before.ends_with("fn ")
            {
                continue;
            }
            let mut depth = 1;
            let Some(close) = text[from..].find(|c| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                depth == 0
            }) else {
                continue;
            };
            let after = &text[from + close + 1..];
            if [".join(", ".path()", ".to_path_buf()"]
                .iter()
                .any(|taken| after.starts_with(taken))
            {
                lines.push(before.matches('\n').count() + 1);
            }
        }
    }
    lines.sort_unstable();
    lines
}

/// **A folder is held for as long as its path is used.**
/// `scratch(name).join("Guild.app")` compiles, drops the folder at the end
/// of the statement and keeps a path into nothing — which the next
/// `create_dir_all` makes again, where tempfile will never look for it.
/// Five update tests did exactly that on the first run of this change,
/// and three of them asserted `!dir.exists()` of a folder that could not
/// have existed either way. `Scratch::within` is the form that holds on.
#[test]
fn no_folder_is_dropped_in_the_expression_that_made_it() {
    let sample = "fn scratch(name: &str) -> Scratch {\n}\n\
                  let app = scratch(name).join(\"Guild.app\");\n\
                  let kept = scratch(name).within(\"Guild.app\");\n\
                  let dir = scratch(name).path().to_path_buf();\n";
    assert_eq!(
        temporaries(sample),
        [3, 5],
        "the check itself finds the shape it is for"
    );

    let wrong: Vec<String> = sources()
        .into_iter()
        .flat_map(|(name, text)| {
            temporaries(&text)
                .into_iter()
                .map(move |line| format!("{name}:{line}"))
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "a folder dropped in the expression that made it:\n{}",
        wrong.join("\n")
    );
}
