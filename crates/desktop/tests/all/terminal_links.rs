//! A link in a terminal's output, found where it is and resolved where it
//! leads — M5.12, task 11.
//!
//! The scanner is a table of cases; the grid is a `Term` with no process
//! behind it, fed by the parser a pty would feed; the disk is a directory
//! under the temporary path, never the process's working directory.

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use guild_desktop::terminal::emulator::GridSize;
use guild_desktop::terminal::links::{self, Link, Resolved, Target};

fn term_with(columns: usize, rows: usize, bytes: &str) -> Term<VoidListener> {
    let mut term = Term::new(
        Config::default(),
        &GridSize::new(columns, rows),
        VoidListener,
    );
    let mut parser: Processor<StdSyncHandler> = Processor::new();
    parser.advance(&mut term, bytes.as_bytes());
    term
}

/// The disk's answer for a joined path, where no test asks it.
fn no_file(_: &links::Found) -> bool {
    false
}

fn at(line: i32, column: usize) -> Point {
    Point::new(Line(line), Column(column))
}

fn file(path: &str, line: u32, column: Option<u32>) -> Target {
    Target::File {
        path: PathBuf::from(path),
        line: Some(line),
        column,
    }
}

/// The text a link covers, by its character offsets.
fn covered(text: &str, link: &Link) -> String {
    text.chars()
        .skip(link.start)
        .take(link.end - link.start)
        .collect()
}

#[test]
fn rustc_s_arrow_is_a_file_at_a_line_and_column() {
    let text = "  --> src/main.rs:12:5";
    let found = links::find(text);

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].target, file("src/main.rs", 12, Some(5)));
    assert_eq!(
        covered(text, &found[0]),
        "src/main.rs:12:5",
        "the arrow is not part of the link"
    );
}

#[test]
fn a_stack_frame_in_parentheses_is_the_path_inside_them() {
    let text = "    at f (/w/a.js:10:5)";
    let found = links::find(text);

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].target, file("/w/a.js", 10, Some(5)));
    assert_eq!(covered(text, &found[0]), "/w/a.js:10:5");
}

#[test]
fn a_url_keeps_its_balanced_parentheses_and_loses_its_full_stop() {
    let cases = [
        (
            "see https://example.com/guild?(a)=1.",
            "https://example.com/guild?(a)=1",
        ),
        ("(https://example.com/x)", "https://example.com/x"),
        (
            "[docs](https://example.com/a_(b)), then",
            "https://example.com/a_(b)",
        ),
        ("\"https://example.com/q\"", "https://example.com/q"),
        ("https://example.com/end?", "https://example.com/end"),
    ];
    for (text, url) in cases {
        let found = links::find(text);
        assert_eq!(found.len(), 1, "{text}: {found:?}");
        assert_eq!(found[0].target, Target::Url(url.to_owned()), "{text}");
        assert_eq!(covered(text, &found[0]), url, "{text}");
    }
}

#[test]
fn only_http_and_https_are_urls_and_file_is_a_path() {
    assert_eq!(
        links::find("http://example.com/a")[0].target,
        Target::Url("http://example.com/a".to_owned())
    );
    for text in [
        "ftp://example.com/a.txt:3",
        "ftp://example.com/a.txt",
        "ftp://example.com/download?file=src/lib.rs",
        "javascript:alert(1)",
        "ssh://host/src/main.rs:4",
        "mailto:someone@example.com",
    ] {
        assert_eq!(links::find(text), Vec::new(), "{text} is not a link");
    }

    let text = "at file:///w/src/a.rs:3:2 here";
    let found = links::find(text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].target, file("/w/src/a.rs", 3, Some(2)));
    assert_eq!(covered(text, &found[0]), "file:///w/src/a.rs:3:2");
    assert_eq!(links::find("file:///w/src/a.rs").len(), 1);
    // And `file://` is followed by an absolute path or by nothing.
    assert_eq!(links::find("file://w/src/a.rs:3"), Vec::new());
    // A scheme is a scheme in capitals too.
    assert_eq!(
        links::find("HTTPS://example.com/a")[0].target,
        Target::Url("HTTPS://example.com/a".to_owned())
    );
}

#[test]
fn a_time_a_port_and_a_word_with_a_colon_are_not_links() {
    for text in [
        "12:30:45",
        "listening on localhost:8080",
        "note:3",
        "ERROR:12",
        // A number with dots is an address or a version, not a file.
        "connect 10.0.0.1:8080",
        "1.5:3",
        // A path after a label is the label's.
        "user@host:src/a.rs:3",
    ] {
        assert_eq!(links::find(text), Vec::new(), "{text} is not a link");
    }
}

#[test]
fn paths_without_lines_keep_only_the_path() {
    for (text, path) in [
        (
            "Замеры и подробности добавлены в отчёт (docs/reports/2026-10-15-rendering.md).",
            "docs/reports/2026-10-15-rendering.md",
        ),
        ("edit src/lib.rs and go", "src/lib.rs"),
        ("[x](docs/a.md)", "docs/a.md"),
        ("see `docs/x.md`", "docs/x.md"),
        ("open Cargo.toml.", "Cargo.toml"),
        ("/abs/dir/x.md", "/abs/dir/x.md"),
        ("docs/x.md.,;:!?", "docs/x.md"),
        ("src/lib.rs:0", "src/lib.rs"),
        ("src/lib.rs:7abc", "src/lib.rs"),
        ("Cargo.toml:7abc", "Cargo.toml"),
        ("Cargo.toml:7ошибка", "Cargo.toml"),
    ] {
        let found = links::find(text);
        assert_eq!(found.len(), 1, "{text}: {found:?}");
        assert_eq!(covered(text, &found[0]), path);
        assert!(
            matches!(&found[0].target, Target::File { path: found, line: None, column: None } if found == Path::new(path))
        );
    }
}

#[test]
fn located_paths_keep_their_line_and_column() {
    assert_eq!(
        links::find("Cargo.toml:7")[0].target,
        file("Cargo.toml", 7, None)
    );
    assert_eq!(
        links::find("Cargo.toml:7:2.")[0].target,
        file("Cargo.toml", 7, Some(2))
    );
    assert_eq!(
        links::find("src/lib.rs:7")[0].target,
        file("src/lib.rs", 7, None)
    );
    assert_eq!(
        links::find("src/lib.rs:7:0")[0].target,
        file("src/lib.rs", 7, None),
        "a column of 0 is no column, and the line stands"
    );
    let text = "src/lib.rs:7: warning";
    let found = links::find(text);
    assert_eq!(found[0].target, file("src/lib.rs", 7, None));
    assert_eq!(covered(text, &found[0]), "src/lib.rs:7");
    // Two on one row are two.
    let two = links::find("./a/b.rs:1 and ../c.py:22:3");
    assert_eq!(
        two.iter().map(|l| l.target.clone()).collect::<Vec<_>>(),
        vec![file("./a/b.rs", 1, None), file("../c.py", 22, Some(3))]
    );
}

#[test]
fn a_url_wrapped_over_three_rows_is_one_link() {
    // Fifty characters on a grid twenty wide: three rows, one line.
    let url = "https://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert_eq!(url.len(), 50);
    let term = term_with(20, 5, &format!("{url} tail"));

    let (text, points) = links::logical_line(&term, at(1, 4));
    assert!(text.starts_with(url), "{text:?}");
    assert_eq!(points.len(), text.chars().count());
    assert_eq!(points[0], at(0, 0));
    assert_eq!(points[49], at(2, 9));

    for point in [at(0, 3), at(1, 10), at(2, 5)] {
        let found = links::at(&term, point, no_file).expect("a link under the point");
        assert_eq!(found.target, Target::Url(url.to_owned()));
        assert_eq!((found.start(), found.end()), (at(0, 0), at(2, 9)));
    }
    assert_eq!(
        links::at(&term, at(2, 12), no_file),
        None,
        "`tail` is no link"
    );
}

#[test]
fn a_wrapped_harness_html_path_routes_to_the_page_unless_it_names_a_source_location() {
    let base = Scratch::new("guild-html-links-");
    let root = fixture(&base);
    let page = root.join(".guild/workflow/full-width-tree-rows/страница.html");
    std::fs::create_dir_all(page.parent().unwrap()).unwrap();
    std::fs::write(&page, "<!doctype html><title>Report</title>").unwrap();
    for suffix in ["", ":12:4"] {
        let term = term_with(40, 10, &format!("see (`{}{suffix}`).", page.display()));
        let target = links::at(&term, at(1, 7), no_file)
            .expect("soft-wrapped HTML path")
            .target;
        let Target::File { path, line, column } = target else {
            panic!("local page must be resolved as a file first")
        };
        assert_eq!(path, page);
        let clicked = links::Clicked { path, line, column };
        let url = clicked.browser_url(&root, &root);
        if suffix.is_empty() {
            assert_eq!(
                url,
                Some(
                    gpui::http_client::Url::from_file_path(page.canonicalize().unwrap())
                        .unwrap()
                        .to_string()
                )
            );
        } else {
            assert_eq!((line, column), (Some(12), Some(4)));
            assert_eq!(url, None, "source locations retain editor navigation");
        }
    }
}

#[test]
fn an_osc8_hyperlink_is_its_uri_over_its_cells() {
    let term = term_with(
        40,
        3,
        "go \x1b]8;;https://example.org/osc8\x1b\\click me\x1b]8;;\x1b\\ after",
    );

    let found = links::at(&term, at(0, 6), no_file).expect("the hyperlink");
    assert_eq!(
        found.target,
        Target::Url("https://example.org/osc8".to_owned())
    );
    assert_eq!(found.runs(), [(at(0, 3), at(0, 10))]);
    assert_eq!(links::at(&term, at(0, 13), no_file), None);
    assert_eq!(links::at(&term, at(0, 0), no_file), None);

    // A hyperlink to a scheme that is not the web's opens nothing.
    let term = term_with(40, 3, "\x1b]8;;ftp://example.org/x\x1b\\get\x1b]8;;\x1b\\");
    assert_eq!(links::at(&term, at(0, 1), no_file), None);
}

/// A project directory with a start directory inside it and a file beside
/// it, in `base` under the temporary path. Its root is **not**
/// canonicalised here: on macOS that path is under `/var`, which is
/// `/private/var`, and `resolve` must see through it.
fn fixture(base: &Path) -> PathBuf {
    let root = base.join("project");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("sub/here.rs"), "").unwrap();
    std::fs::write(root.join("only-root.rs"), "").unwrap();
    std::fs::write(base.join("outside.txt"), "").unwrap();
    root
}

#[test]
fn a_path_resolves_inside_the_project_outside_it_or_not_at_all() {
    let base = Scratch::new("guild-links-");
    let root = fixture(&base);
    let cwd = root.join("sub");
    let real_base = base.canonicalize().unwrap();

    let resolve = |path: &str, cwd: &Path| links::resolve(Path::new(path), cwd, &root);

    assert_eq!(
        resolve("here.rs", &cwd),
        Some(Resolved::Inside(PathBuf::from("sub/here.rs"))),
        "from the start directory first"
    );
    assert_eq!(
        resolve("only-root.rs", &cwd),
        Some(Resolved::Inside(PathBuf::from("only-root.rs"))),
        "then from the root"
    );
    assert_eq!(
        resolve("../outside.txt", &root),
        Some(Resolved::Outside(real_base.join("outside.txt")))
    );
    assert_eq!(
        resolve(
            real_base.join("project/sub/here.rs").to_str().unwrap(),
            &cwd
        ),
        Some(Resolved::Inside(PathBuf::from("sub/here.rs"))),
        "an absolute path into the project is a path within it"
    );
    assert_eq!(resolve("missing.rs", &cwd), None);
    assert_eq!(resolve("sub", &root), None, "a directory is not a file");
}

/// A press acts on the link under it **now**, not on the one underlined
/// when the pointer last moved: output moves text under a still pointer
/// and a program redraws in place, and the underline is kept against a
/// grid point. Found by review of task 12.
#[test]
fn a_press_acts_on_the_link_under_it_now_and_not_on_the_one_underlined_before() {
    let underlined = links::at(
        &term_with(60, 3, "see https://example.com/old now"),
        at(0, 8),
        no_file,
    );
    assert!(underlined.is_some());

    // The same cells, redrawn with another address.
    let redrawn = term_with(60, 3, "see https://example.com/new now");
    assert_eq!(
        links::pressed(
            links::at(&redrawn, at(0, 8), no_file),
            underlined.as_ref(),
            |_| true
        ),
        Some(Target::Url("https://example.com/new".to_owned()))
    );
    // The same cells, holding no link any more.
    let plain = term_with(60, 3, "nothing to open under the pointer");
    assert_eq!(
        links::pressed(
            links::at(&plain, at(0, 8), no_file),
            underlined.as_ref(),
            |_| true
        ),
        None
    );

    // The file underlined, deleted since: still the link that was promised,
    // so the press is its and the window says *No such file*.
    let gone = links::at(
        &term_with(60, 3, "  --> src/gone.rs:3:1"),
        at(0, 8),
        no_file,
    );
    assert_eq!(
        links::pressed(gone.clone(), gone.as_ref(), |_| false),
        Some(file("src/gone.rs", 3, Some(1)))
    );
    // Another file in those cells is asked of the disk like any other.
    let other = links::at(
        &term_with(60, 3, "  --> src/else.rs:3:1"),
        at(0, 8),
        no_file,
    );
    assert_eq!(
        links::pressed(other.clone(), gone.as_ref(), |_| false),
        None
    );
    assert_eq!(
        links::pressed(other.clone(), gone.as_ref(), |_| true),
        Some(file("src/else.rs", 3, Some(1)))
    );
    // Nothing underlined — an Output terminal that never heard `cmd`.
    assert_eq!(
        links::pressed(other, None, |_| true),
        Some(file("src/else.rs", 3, Some(1)))
    );
}

#[test]
fn osc8_file_uris_decode_before_resolving_and_keep_the_label() {
    let base = Scratch::new("guild-osc8-");
    let root = fixture(&base);
    std::fs::write(root.join("отчёт a.md"), "report").unwrap();
    for (uri, expected, line, column) in [
        (
            format!("file://{}/only-root.rs", root.display()),
            "only-root.rs",
            None,
            None,
        ),
        (
            format!(
                "file://{}/%D0%BE%D1%82%D1%87%D1%91%D1%82%20a.md",
                root.display()
            ),
            "отчёт a.md",
            None,
            None,
        ),
        (
            format!("file://localhost{}/only-root.rs:3:2", root.display()),
            "only-root.rs",
            Some(3),
            Some(2),
        ),
    ] {
        let term = term_with(
            80,
            3,
            &format!("go \x1b]8;;{uri}\x1b\\отчёт\x1b]8;;\x1b\\ after"),
        );
        let found = links::at(&term, at(0, 5), no_file).expect("OSC 8 file link");
        assert_eq!(found.runs(), [(at(0, 3), at(0, 7))]);
        let target = found.target;
        assert_eq!(
            target,
            Target::File {
                path: root.join(expected),
                line,
                column
            }
        );
        let Target::File { path, .. } = target else {
            panic!("a file")
        };
        assert_eq!(
            links::resolve(&path, &root, &root),
            Some(Resolved::Inside(expected.into()))
        );
    }
    for uri in [
        "file://otherhost/w/a.rs",
        "file:///w/%FF.md",
        "ssh://host/a.rs",
    ] {
        let term = term_with(
            80,
            3,
            &format!("\x1b]8;;{uri}\x1b\\src/lib.rs\x1b]8;;\x1b\\"),
        );
        assert_eq!(links::at(&term, at(0, 3), no_file), None, "{uri}");
    }
}

#[test]
fn bare_candidates_only_take_a_press_when_the_file_exists_or_was_underlined() {
    let base = Scratch::new("guild-bare-links-");
    let root = fixture(&base);
    for text in [
        "e.g.",
        "v0.162.1",
        "node.js",
        "docs/missing.md",
        "sub/",
        "only-root.rs",
    ] {
        let now = links::at(&term_with(80, 3, text), at(0, 1), no_file);
        assert!(now.is_some(), "{text} is a candidate");
        let pressed = links::pressed(now.clone(), None, |target| {
            let Target::File { path, .. } = target else {
                return false;
            };
            links::resolve(path, &root, &root).is_some()
        });
        assert_eq!(pressed.is_some(), text == "only-root.rs", "{text}");
        if text == "only-root.rs" {
            std::fs::remove_file(root.join(text)).unwrap();
            assert!(links::pressed(now.clone(), now.as_ref(), |_| false).is_some());
            assert!(links::pressed(now, None, |_| false).is_none());
        }
    }
}

#[test]
fn arbitrary_opaque_names_are_paths_not_foreign_uri_tokens() {
    // Accepted limitation: design.md, "Scheme guard simplified after code-3".
    // Only hierarchical and listed opaque schemes suppress file candidates.
    for (text, first, last) in [
        (
            "org.example:123?file=src/lib.rs",
            "org.example:123",
            "src/lib.rs",
        ),
        (
            "org.example:123:4?file=src/lib.rs",
            "org.example:123:4",
            "src/lib.rs",
        ),
        ("org.example:0?file=src/lib.rs", "org.example", "src/lib.rs"),
        (
            "org.example:123?src/lib.rs",
            "org.example:123",
            "src/lib.rs",
        ),
        ("org.example:123?foo.txt", "org.example:123", "foo.txt"),
        (
            "org.example:123?'src/lib.rs'",
            "org.example:123",
            "src/lib.rs",
        ),
        (
            "org.example:123?)src/lib.rs",
            "org.example:123",
            "src/lib.rs",
        ),
        (
            "org.example:123?`src/lib.rs`",
            "org.example:123",
            "src/lib.rs",
        ),
    ] {
        let found = links::find(text);
        assert_eq!(
            found.iter().map(|l| covered(text, l)).collect::<Vec<_>>(),
            [first, last],
            "{text}"
        );
        assert!(
            found
                .iter()
                .all(|l| matches!(l.target, Target::File { .. }))
        );
    }
}

#[test]
fn foreign_uri_tokens_end_at_text_delimiters() {
    for delimiter in [" ", "\t", "\n", "\"", "'", "`", "<", ">"] {
        // Quotes opening a query end the token too; accepted in design.md,
        // "Scheme guard simplified after code-3".
        let text = format!("ftp://host/?file={delimiter}src/a.rs");
        let found = links::find(&text);
        assert_eq!(found.len(), 1, "{text}: {found:?}");
        assert_eq!(covered(&text, &found[0]), "src/a.rs");
    }
    let text = r#"{"url":"ftp://h/x","path":"src/a.rs"}"#;
    let found = links::find(text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(covered(text, &found[0]), "src/a.rs");
}

#[test]
fn hierarchical_and_listed_opaque_schemes_hide_embedded_links() {
    for scheme in [
        "ftp://",
        "ssh://",
        "a+B-c.d://",
        "MAILTO:",
        "JavaScript:",
        "VBSCRIPT:",
        "data:",
        "tel:",
        "urn:",
        "about:",
    ] {
        for body in ["host/?file=src/a.rs", "host/?next=https://example.org/a"] {
            let text = format!("{scheme}{body}");
            assert!(links::find(&text).is_empty(), "{text}");
        }
    }
}

#[test]
fn a_long_non_link_does_not_block_the_terminal() {
    let text = "a-".repeat(50_000);
    let started = std::time::Instant::now();
    assert!(links::find(&text).is_empty());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "a single logical line took {:?}",
        started.elapsed()
    );
}

#[test]
fn grep_locations_end_before_the_matched_content() {
    for (text, expected, span) in [
        (
            "src/a.rs:12:text",
            file("src/a.rs", 12, None),
            "src/a.rs:12",
        ),
        (
            "Cargo.toml:1:[workspace]",
            file("Cargo.toml", 1, None),
            "Cargo.toml:1",
        ),
        (
            "README.md:7:# Heading",
            file("README.md", 7, None),
            "README.md:7",
        ),
        (
            "Cargo.toml:1:2:[workspace]",
            file("Cargo.toml", 1, Some(2)),
            "Cargo.toml:1:2",
        ),
    ] {
        let found = links::find(text);
        assert_eq!(found.len(), 1, "{text}");
        assert_eq!(found[0].target, expected);
        assert_eq!(covered(text, &found[0]), span);
    }
}

#[test]
fn located_paths_stop_at_closing_delimiters() {
    for (text, expected) in [
        (
            r#"["Cargo.toml:7","README.md:8"]"#,
            vec![
                (file("Cargo.toml", 7, None), "Cargo.toml:7"),
                (file("README.md", 8, None), "README.md:8"),
            ],
        ),
        (
            "[Cargo.toml:7](Cargo.toml)",
            vec![
                (file("Cargo.toml", 7, None), "Cargo.toml:7"),
                (
                    Target::File {
                        path: "Cargo.toml".into(),
                        line: None,
                        column: None,
                    },
                    "Cargo.toml",
                ),
            ],
        ),
    ] {
        let found = links::find(text);
        assert_eq!(found.len(), expected.len(), "{text}: {found:?}");
        for (link, (target, span)) in found.iter().zip(expected) {
            assert_eq!(link.target, target, "{text}");
            assert_eq!(covered(text, link), span, "{text}");
        }
    }
}

/// Who shows a file a `cmd`-click resolved to: Guild for what a tab can
/// show — text, Markdown, an image it previews, SVG included — and the
/// application macOS opens it with for everything else; a program is
/// refused rather than run.
#[test]
fn what_guild_cannot_show_goes_to_the_system_and_a_program_is_refused() {
    use guild_desktop::terminal::links::{Shown, shown_by};
    use std::os::unix::fs::PermissionsExt as _;

    let dir = Scratch::new("guild-links-shown-");
    let write = |name: &str, bytes: &[u8]| {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    };

    let text = write("notes.txt", b"one\ntwo\nthree\n");
    let markdown = write("README.md", "# Привет\n".as_bytes());
    let svg = write(
        "qr.svg",
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"/>",
    );
    // An image Guild previews is Guild's whatever its bytes look like.
    let png = write("shot.png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
    let pdf = write("report.pdf", b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj\0");
    let video = write("clip.mp4", b"\0\0\0\x18ftypmp42\0\0\0\0");
    let latin1 = write("old.txt", b"caf\xe9\n");

    assert_eq!(shown_by(&text), Shown::InGuild);
    assert_eq!(shown_by(&markdown), Shown::InGuild);
    assert_eq!(shown_by(&svg), Shown::InGuild);
    assert_eq!(shown_by(&png), Shown::InGuild);
    assert_eq!(shown_by(&pdf), Shown::BySystem);
    assert_eq!(shown_by(&video), Shown::BySystem);
    assert_eq!(
        shown_by(&latin1),
        Shown::BySystem,
        "not UTF-8 is not text, the tab's own rule"
    );

    // Past the open limit a tab says the file is too large, as it did;
    // bytes that are not text at the start go to the system all the same,
    // or a long video would never reach its player.
    let limit = guild_desktop::file_tab::OPEN_LIMIT as usize;
    let large_text = write("huge.log", &vec![b'a'; limit + 1]);
    let mut movie = b"\0\0\0\x18ftypqt  ".to_vec();
    movie.resize(limit + 1, 7);
    let large_movie = write("long.mov", &movie);
    assert_eq!(shown_by(&large_text), Shown::InGuild);
    assert_eq!(shown_by(&large_movie), Shown::BySystem);

    // A program is something the system would run, not show.
    let macho = write("tool", b"\xcf\xfa\xed\xfe\x07\0\0\x01\0\0\0\0");
    let mut perms = std::fs::metadata(&macho).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&macho, perms).unwrap();
    let elsewhere = write("plugin.bundle-bin", b"\xcf\xfa\xed\xfe\x07\0\0\x01");
    let executable = write("run-me", b"\x7fELF\x02\x01\x01\0\0\0");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(shown_by(&macho), Shown::Refused);
    assert_eq!(shown_by(&elsewhere), Shown::Refused, "Mach-O by its bytes");
    assert_eq!(
        shown_by(&executable),
        Shown::Refused,
        "an executable with no extension"
    );
    // An executable bit alone, on a file with a type, is not a program:
    // a PDF copied off a FAT volume keeps 0o777.
    std::fs::set_permissions(&pdf, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(shown_by(&pdf), Shown::BySystem);

    assert_eq!(shown_by(&dir.join("gone.pdf")), Shown::Gone);
}

/// What macOS would launch or install when opened is never handed to it,
/// whatever the bytes inside say: Terminal runs a `.command` with the
/// person's privileges, Installer installs a `.pkg`. Only text Guild shows
/// itself — a UTF-8 script — still opens, read-only, in a tab.
#[test]
fn nothing_macos_would_launch_or_install_is_handed_to_it() {
    use guild_desktop::terminal::links::{Shown, shown_by};
    use std::os::unix::fs::PermissionsExt as _;

    let dir = Scratch::new("guild-links-launch-");
    let write = |name: &str, bytes: &[u8], mode: u32| {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path
    };

    // A shell script with one Latin-1 byte in a comment is not text, and
    // the extension said Terminal.
    let script = write(
        "deploy.command",
        b"#!/bin/sh\n# caf\xe9\ntouch /tmp/x\n",
        0o755,
    );
    assert_eq!(shown_by(&script), Shown::Refused, "an executable .command");
    let plain = write("again.command", b"#!/bin/sh\n# caf\xe9\n", 0o644);
    assert_eq!(shown_by(&plain), Shown::Refused, "the extension alone");
    let tool = write("build.tool", b"\0#!/bin/sh\n", 0o644);
    assert_eq!(shown_by(&tool), Shown::Refused);
    let package = write("update.pkg", b"xar!\0\x1c\0\x01\0\0\0\0", 0o644);
    assert_eq!(shown_by(&package), Shown::Refused, "Installer installs it");
    let jar = write("app.jar", b"PK\x03\x04\x14\0\0\0\x08\0", 0o644);
    assert_eq!(shown_by(&jar), Shown::Refused, "Jar Launcher runs it");
    let settings = write("evil.terminal", b"bplist00\xd4\x01\x02\0", 0o644);
    assert_eq!(shown_by(&settings), Shown::Refused, "a CommandString runs");
    let location = write("there.fileloc", b"bplist00\xd1\x01\x02\0", 0o644);
    assert_eq!(
        shown_by(&location),
        Shown::Refused,
        "it opens what it points at"
    );

    // An executable bit and a shebang make a script of any name.
    let disguised = write("notes.dat", b"#!/bin/sh\n# caf\xe9\n", 0o755);
    assert_eq!(shown_by(&disguised), Shown::Refused);
    let document = write("letter.pdf", b"#!/bin/sh\n%PDF-1.7\n", 0o755);
    assert_eq!(
        shown_by(&document),
        Shown::Refused,
        "a document's extension does not get a script past"
    );

    // Text is Guild's, read-only: nothing runs.
    let readable = write(
        "start.command",
        "#!/bin/sh\necho привет\n".as_bytes(),
        0o755,
    );
    assert_eq!(shown_by(&readable), Shown::InGuild);
    let shell = write("setup.sh", b"#!/bin/sh\necho hi\n", 0o755);
    assert_eq!(shown_by(&shell), Shown::InGuild);
}

/// A document Guild cannot show goes to its application by its format,
/// before the bytes are asked whether they are text: a PDF written by
/// hand is all ASCII and still a PDF. Text formats stay Guild's.
#[test]
fn a_document_goes_to_its_application_even_when_its_bytes_are_ascii() {
    use guild_desktop::terminal::links::{Shown, shown_by};

    let dir = Scratch::new("guild-links-document-");
    let write = |name: &str, bytes: &[u8]| {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    };
    let pdf = write(
        "plain.pdf",
        b"%PDF-1.1\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
          2 0 obj << /Type /Pages /Kids [] /Count 0 >> endobj\n\
          trailer << /Root 1 0 R >>\n%%EOF\n",
    );
    assert_eq!(shown_by(&pdf), Shown::BySystem);
    let upper = write("SCAN.PDF", b"%PDF-1.4\n%%EOF\n");
    assert_eq!(
        shown_by(&upper),
        Shown::BySystem,
        "the case of the extension"
    );
    let rtf = write("letter.rtf", b"{\\rtf1\\ansi Hello}");
    assert_eq!(shown_by(&rtf), Shown::BySystem);

    for (name, bytes) in [
        ("data.json", &b"{\"a\": 1}\n"[..]),
        ("table.csv", b"a,b\n1,2\n"),
        ("README.md", b"# Title\n"),
        ("notes.txt", b"plain\n"),
        ("logo.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
    ] {
        assert_eq!(shown_by(&write(name, bytes)), Shown::InGuild, "{name}");
    }
}

/// A FIFO at the path has no writer, and opening it to read waits for one.
/// This covers a pipe already at the path when the click is classified —
/// it is refused before anything opens it. It does not reproduce a file
/// replaced by a pipe between two reads: that race is closed by reading
/// everything through one non-blocking, checked handle, not by this test.
#[test]
fn a_pipe_at_the_path_does_not_hang_the_click() {
    use guild_desktop::terminal::links::{Shown, shown_by};

    let dir = Scratch::new("guild-links-fifo-");
    for name in ["pipe.mov", "pipe", "pipe.pdf", "pipe.png"] {
        let path = dir.join(name);
        let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o644) }, 0);
        let (tx, rx) = std::sync::mpsc::channel();
        let asked = path.clone();
        std::thread::spawn(move || tx.send(shown_by(&asked)));
        let shown = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("{name}: classification hung on a FIFO"));
        assert_ne!(
            shown,
            Shown::BySystem,
            "{name}: a pipe is nobody's document"
        );
        assert_ne!(shown, Shown::Refused, "{name}");
    }
}

/// Round 2 of the same guard: a fat Mach-O for 64-bit offsets, a Finder
/// alias — which macOS opens by opening its original, an application
/// included — and Flat OpenDocument, XML that is still an office document.
#[test]
fn a_fat64_program_and_an_alias_are_refused_and_flat_odf_is_a_document() {
    use guild_desktop::terminal::links::{Shown, shown_by};

    let dir = Scratch::new("guild-links-round2-");
    let write = |name: &str, bytes: &[u8]| {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    };

    let fat64 = write("tool.bin", b"\xca\xfe\xba\xbf\0\0\0\x02\x01\0\0\x07");
    assert_eq!(shown_by(&fat64), Shown::Refused, "FAT_MAGIC_64");
    let swapped = write("tool2.bin", b"\xbf\xba\xfe\xca\x02\0\0\0");
    assert_eq!(shown_by(&swapped), Shown::Refused, "FAT_CIGAM_64");

    // A bookmark file's header, as Finder writes an alias.
    let mut bookmark = b"book\0\0\0\0mark\0\0\0\0".to_vec();
    bookmark.extend_from_slice(&[0x38, 0, 0, 0, 0x30, 0, 0, 0]);
    bookmark.resize(512, 0);
    let alias = write("Report alias", &bookmark);
    assert_eq!(
        shown_by(&alias),
        Shown::Refused,
        "an alias opens its original"
    );
    let named = write("report.pdf", &bookmark);
    assert_eq!(
        shown_by(&named),
        Shown::Refused,
        "a document's name does not get an alias past"
    );

    for name in ["minutes.fodt", "budget.fods", "talk.fodp", "chart.fodg"] {
        let flat = write(
            name,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document>Привет</office:document>\n"
                .as_bytes(),
        );
        assert_eq!(shown_by(&flat), Shown::BySystem, "{name}");
    }
}
