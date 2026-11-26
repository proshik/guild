//! Searching a project for text — M8.7's model, from outside.
//!
//! Every one of these is an acceptance criterion of task 2 of the plan.
//! The tree is built on disk because that is what is being tested: the
//! walk, the ignore rules and the matcher together are the thing, and a
//! fake filesystem would only be testing the fake.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::scratch::Scratch;
use guild_desktop::search::{self, Query, Refused};

/// A tree of our own, named after the test and this process — the
/// convention every filesystem test in this crate follows, because a
/// shared name is two tests deleting each other's files under `just
/// check`.
fn tree(name: &str, files: &[(&str, &[u8])]) -> Scratch {
    let root = Scratch::new(&format!("guild-search-{name}-"));
    for (path, bytes) in files {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("a directory");
        }
        std::fs::write(&full, bytes).expect("a file");
    }
    root
}

/// Everything a search of this tree finds, in path order so a test can
/// say what it expects.
fn found(root: &Path, query: &Query) -> Vec<search::Hit> {
    let stop = AtomicBool::new(false);
    let mut hits = Vec::new();
    search::search(root, query, &stop, |batch| hits.extend(batch)).expect("the query compiles");
    hits.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    hits
}

/// A plain query: the text and nothing switched on.
fn plain(text: &str) -> Query {
    Query {
        text: text.to_owned(),
        ..Query::default()
    }
}

#[test]
fn a_hit_is_a_line_of_a_file_and_the_place_in_it() {
    let root = tree(
        "plain",
        &[("src/one.rs", b"fn alpha() {}\nfn beta() {}\n" as &[u8])],
    );

    let hits = found(&root, &plain("beta"));

    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(
        hit.path,
        PathBuf::from("src/one.rs"),
        "relative to the root"
    );
    assert_eq!(hit.line, 2, "one-based, as an editor counts");
    assert_eq!(hit.text, "fn beta() {}");
    assert_eq!(
        &hit.text[hit.ranges[0].clone()],
        "beta",
        "the range points at the match inside the text the row draws"
    );
}

#[test]
fn case_matters_only_when_it_is_asked_to() {
    let root = tree("case", &[("a.txt", b"Alpha\nalpha\n" as &[u8])]);

    assert_eq!(found(&root, &plain("alpha")).len(), 2, "both, by default");

    let exact = Query {
        case_sensitive: true,
        ..plain("alpha")
    };
    let hits = found(&root, &exact);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 2);
}

#[test]
fn a_whole_word_is_not_a_substring() {
    let root = tree("word", &[("a.txt", b"cat\nconcatenate\n" as &[u8])]);

    assert_eq!(found(&root, &plain("cat")).len(), 2);

    let whole = Query {
        whole_word: true,
        ..plain("cat")
    };
    let hits = found(&root, &whole);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 1);
}

#[test]
fn a_regular_expression_is_one_only_when_it_is_asked_to_be() {
    let root = tree("regex", &[("a.txt", b"a.c\nabc\n" as &[u8])]);

    let literal = found(&root, &plain("a.c"));
    assert_eq!(literal.len(), 1, "a dot is a dot until told otherwise");
    assert_eq!(literal[0].line, 1);

    let pattern = Query {
        regular_expression: true,
        ..plain("a.c")
    };
    assert_eq!(found(&root, &pattern).len(), 2);
}

#[test]
fn a_mask_narrows_to_the_files_it_names() {
    let root = tree(
        "mask",
        &[
            ("src/a.rs", b"needle\n" as &[u8]),
            ("src/a.txt", b"needle\n" as &[u8]),
            ("notes.md", b"needle\n" as &[u8]),
        ],
    );

    assert_eq!(
        found(&root, &plain("needle")).len(),
        3,
        "no mask, all of it"
    );

    let only_rust = Query {
        mask: "*.rs".to_owned(),
        ..plain("needle")
    };
    let hits = found(&root, &only_rust);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, PathBuf::from("src/a.rs"));

    let two = Query {
        mask: "*.rs,*.md".to_owned(),
        ..plain("needle")
    };
    assert_eq!(found(&root, &two).len(), 2, "a comma is another glob");
}

#[test]
fn what_git_ignores_is_not_searched_and_git_itself_is_never_entered() {
    let root = tree(
        "ignored",
        &[
            (".gitignore", b"target/\n" as &[u8]),
            ("src/a.rs", b"needle\n" as &[u8]),
            ("target/debug/a.rs", b"needle\n" as &[u8]),
            (".git/config", b"needle\n" as &[u8]),
        ],
    );

    let hits = found(&root, &plain("needle"));

    assert_eq!(
        hits.iter().map(|h| h.path.clone()).collect::<Vec<_>>(),
        vec![PathBuf::from("src/a.rs")],
        "a search of target/ is not a search, and .git is never a place to look"
    );
}

#[test]
fn a_binary_file_is_not_searched() {
    let root = tree(
        "binary",
        &[
            ("a.txt", b"needle\n" as &[u8]),
            ("b.bin", b"needle\x00\x00\x00binary\n" as &[u8]),
        ],
    );

    let hits = found(&root, &plain("needle"));

    assert_eq!(
        hits.len(),
        1,
        "a hit inside a png is a line nobody can draw"
    );
    assert_eq!(hits[0].path, PathBuf::from("a.txt"));
}

#[test]
fn a_pattern_that_will_not_compile_is_refused_in_the_regex_crates_own_words() {
    let root = tree("bad", &[("a.txt", b"anything\n" as &[u8])]);
    let broken = Query {
        regular_expression: true,
        ..plain("a(")
    };

    let stop = AtomicBool::new(false);
    let answer = search::search(&root, &broken, &stop, |_| {});

    match answer {
        Err(Refused::Pattern(said)) => assert!(
            said.contains('('),
            "the regex crate's words, not ours: {said}"
        ),
        other => panic!("a broken pattern must be refused, not silently empty: {other:?}"),
    }
}

#[test]
fn a_mask_that_will_not_compile_is_refused_too() {
    let root = tree("badmask", &[("a.txt", b"needle\n" as &[u8])]);
    let broken = Query {
        mask: "[".to_owned(),
        ..plain("needle")
    };

    let stop = AtomicBool::new(false);
    let answer = search::search(&root, &broken, &stop, |_| {});

    assert!(
        matches!(answer, Err(Refused::Mask(_))),
        "a mask nobody can parse is a refusal, not a search of nothing"
    );
}

#[test]
fn a_long_line_is_cut_around_its_match() {
    // A minified file is one line, and task 0 measured one at 295,067
    // bytes in the wild. Cutting at the start would show a screenful of
    // it with the match off the end.
    let mut line = "x".repeat(4000);
    line.push_str("needle");
    line.push_str(&"y".repeat(4000));
    line.push('\n');
    let root = tree("long", &[("a.txt", line.as_bytes())]);

    let hits = found(&root, &plain("needle"));

    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert!(hit.cut, "the row says it is not the whole line");
    assert!(
        hit.text.len() <= search::LINE_LIMIT + 16,
        "cut to something a row can draw, not {} bytes",
        hit.text.len()
    );
    assert_eq!(
        &hit.text[hit.ranges[0].clone()],
        "needle",
        "and the range still points at the match inside what is drawn"
    );
}

#[test]
fn a_line_is_cut_on_a_character_boundary() {
    // GPUI refuses a run that starts inside a character, and a Cyrillic
    // comment in a long line is exactly where that would happen.
    let mut line = "щ".repeat(4000);
    line.push_str("needle");
    line.push_str(&"ю".repeat(4000));
    line.push('\n');
    let root = tree("boundary", &[("a.txt", line.as_bytes())]);

    let hits = found(&root, &plain("needle"));

    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert!(hit.text.is_char_boundary(0));
    assert_eq!(&hit.text[hit.ranges[0].clone()], "needle");
}

#[test]
fn the_search_stops_at_the_cap_and_says_it_stopped() {
    let many: String = (0..(search::PER_FILE * 3))
        .map(|i| format!("needle {i}\n"))
        .collect();
    let root = tree("cap", &[("a.txt", many.as_bytes())]);

    let stop = AtomicBool::new(false);
    let mut hits = Vec::new();
    let summary = search::search(&root, &plain("needle"), &stop, |batch| hits.extend(batch))
        .expect("the query compiles");

    assert_eq!(
        hits.len(),
        search::PER_FILE,
        "one file cannot fill the panel by itself"
    );
    assert!(
        summary.capped,
        "a count with no note is a lie about the repository"
    );
}

#[test]
fn a_search_that_is_told_to_stop_gives_up_where_it_is() {
    let root = tree(
        "stop",
        &[
            ("a.txt", b"needle\n" as &[u8]),
            ("b.txt", b"needle\n" as &[u8]),
            ("c.txt", b"needle\n" as &[u8]),
        ],
    );

    let stop = AtomicBool::new(true);
    let mut hits = Vec::new();
    let summary = search::search(&root, &plain("needle"), &stop, |batch| hits.extend(batch))
        .expect("the query compiles");

    assert!(
        hits.is_empty(),
        "a query nobody wants any more costs nothing"
    );
    assert!(summary.stopped);
}

#[test]
fn nothing_typed_is_not_a_search() {
    let root = tree("empty", &[("a.txt", b"needle\n" as &[u8])]);

    assert!(plain("").is_empty());
    assert!(
        found(&root, &plain("")).is_empty(),
        "an empty query matches every line of every file, which is not what anybody meant"
    );
}

/// **`.gitignore` is obeyed in a directory that is not a repository.**
///
/// `ignore`'s default is ripgrep's: git's rules apply only inside a git
/// repository. Guild's file tree has never made that distinction — it
/// dims what `.gitignore` names wherever it finds one — and decision 4
/// of the plan is that a search looks where the tree looks. Without
/// this, a project a person has not run `git init` in would have its
/// `node_modules` searched while the tree shows it greyed out.
#[test]
fn an_ignore_file_is_obeyed_without_a_repository_around_it() {
    let root = tree(
        "norepo",
        &[
            (".gitignore", b"vendor/\n" as &[u8]),
            ("src/a.rs", b"needle\n" as &[u8]),
            ("vendor/b.rs", b"needle\n" as &[u8]),
        ],
    );

    let hits = found(&root, &plain("needle"));

    assert_eq!(
        hits.iter().map(|h| h.path.clone()).collect::<Vec<_>>(),
        vec![PathBuf::from("src/a.rs")],
        "the tree dims it, so the search skips it — one rule, not two"
    );
}

/// **A scope narrows where it looks and changes nothing about what it
/// answers.** M8.8.3: *Find in Files…* on a tree row scopes the search
/// to that directory, and the hits stay relative to the **project** —
/// that path is the key every other panel of the window uses, and a row
/// that came back relative to the scope would open the wrong file.
#[test]
fn a_scope_narrows_the_walk_and_leaves_the_paths_alone() {
    let root = tree(
        "scope",
        &[
            ("src/a.rs", b"needle\n" as &[u8]),
            ("src/deep/b.rs", b"needle\n" as &[u8]),
            ("docs/c.md", b"needle\n" as &[u8]),
        ],
    );

    let mut query = plain("needle");
    query.scope = Some(PathBuf::from("src"));
    let hits = found(&root, &query);

    assert_eq!(
        hits.iter().map(|h| h.path.clone()).collect::<Vec<_>>(),
        vec![PathBuf::from("src/a.rs"), PathBuf::from("src/deep/b.rs")],
        "inside the scope and below it, and the paths are the project's"
    );

    // And no scope is the whole project, which is what `cmd-shift-f`
    // asks by itself.
    assert_eq!(found(&root, &plain("needle")).len(), 3);
}

/// **What is searched is what the Explorer shows, except `.guild`.**
///
/// M5.11, decision 3. `hidden(true)` skipped every dotfile the tree draws
/// — `.github/`, `.env.example` — against M8.7's decision 4. `.git` is
/// never a place to look, and neither is `.guild`: it is Guild's own
/// record and holds the query itself in `state.json`, so a search that
/// read it would find its own words in every project.
#[test]
fn a_dotfile_the_tree_shows_is_searched_and_guilds_own_state_is_not() {
    let root = tree(
        "dotfiles",
        &[
            ("src/a.rs", b"needle\n" as &[u8]),
            (".github/workflows/ci.yml", b"needle\n" as &[u8]),
            (".env.example", b"needle\n" as &[u8]),
            (".guild/state.json", b"{\"query\": \"needle\"}\n" as &[u8]),
            (".git/config", b"needle\n" as &[u8]),
        ],
    );

    let hits = found(&root, &plain("needle"));

    assert_eq!(
        hits.iter().map(|h| h.path.clone()).collect::<Vec<_>>(),
        vec![
            PathBuf::from(".env.example"),
            PathBuf::from(".github/workflows/ci.yml"),
            PathBuf::from("src/a.rs"),
        ],
        "the dotfiles the tree draws, and neither .git nor .guild"
    );
}

/// **A change the search cannot have seen does not make it stale.**
///
/// M5.11, decision 4. A path outside the scope is not where the search
/// looked, and `.git` and `.guild` are never looked in — `.guild` least of
/// all, because the window writes `state.json` on ordinary gestures and
/// every tab switch would otherwise re-ask every search.
#[test]
fn a_change_outside_the_scope_or_inside_git_or_guild_does_not_touch_a_search() {
    let mut scoped = plain("needle");
    scoped.scope = Some(PathBuf::from("docs"));
    assert!(!search::touches(&scoped, &[PathBuf::from("src/a.rs")]));
    assert!(search::touches(&scoped, &[PathBuf::from("docs/x.md")]));

    let whole = plain("needle");
    assert!(!search::touches(
        &whole,
        &[PathBuf::from(".guild/state.json")]
    ));
    assert!(!search::touches(&whole, &[PathBuf::from(".git/index")]));
    assert!(search::touches(&whole, &[PathBuf::from("src/a.rs")]));
    assert!(
        search::touches(
            &whole,
            &[
                PathBuf::from(".guild/state.json"),
                PathBuf::from("src/a.rs")
            ]
        ),
        "one path that matters is enough"
    );
}

/// **A refresh waits for the disk to be quiet, and for the walk, and not
/// for ever.** 300 ms without a batch, or two seconds after the answer
/// went stale — and never over a walk that is still running, which an
/// agent writing for a minute would otherwise restart every two seconds.
#[test]
fn a_refresh_waits_for_quiet_and_for_the_walk_but_not_for_ever() {
    use std::time::{Duration, Instant};

    let now = Instant::now();
    let ago = |ms: u64| now - Duration::from_millis(ms);

    // One batch, 100 ms ago: not quiet yet.
    assert!(!search::refresh_due(ago(100), ago(100), now, false));
    // One batch, 300 ms ago: quiet.
    assert!(search::refresh_due(ago(300), ago(300), now, false));
    // Batches every 100 ms for two seconds: never quiet, but stale long
    // enough.
    assert!(search::refresh_due(ago(2000), ago(100), now, false));
    assert!(!search::refresh_due(ago(1900), ago(100), now, false));

    // None of them while a walk runs.
    assert!(!search::refresh_due(ago(100), ago(100), now, true));
    assert!(!search::refresh_due(ago(300), ago(300), now, true));
    assert!(!search::refresh_due(ago(2000), ago(100), now, true));

    assert_eq!(search::QUIET, Duration::from_millis(300));
    assert_eq!(search::STALE_AT_MOST, Duration::from_secs(2));
}

/// **A click in the pause lands where the text is now.** A hit whose file
/// has changed since the answer is looked for again in that one file: the
/// line nearest its old one holding its own text, or failing that the
/// query's match — and nothing when neither is there.
#[test]
fn a_click_on_a_stale_hit_finds_its_match_where_the_text_moved() {
    let root = tree(
        "relocate",
        &[("docs/a.md", b"# Docs\n\nneedle in docs\n" as &[u8])],
    );
    let query = plain("needle");
    let hits = found(&root, &query);
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.line, 3);

    // Two lines prepended: the text moved two lines down.
    assert_eq!(
        search::relocate("one\ntwo\n# Docs\n\nneedle in docs\n", &query, hit),
        Some((5, 0))
    );
    // A second needle on line 1 is nearer the old line, and the hit's own
    // text still wins.
    assert_eq!(
        search::relocate("needle\ntwo\n# Docs\n\nneedle in docs\n", &query, hit),
        Some((5, 0))
    );
    // The hit's own text on two lines: the nearer one to the old line,
    // not the first one in the file.
    assert_eq!(
        search::relocate("needle in docs\n# Docs\n\nneedle in docs\n", &query, hit),
        Some((4, 0))
    );
    // The hit's text gone, the query's match elsewhere: that match, at
    // its column in characters.
    assert_eq!(
        search::relocate("# Docs\n\nin docs\n  ёж needle\n", &query, hit),
        Some((4, 5))
    );
    // The word removed: nothing to land on.
    assert_eq!(
        search::relocate("one\ntwo\n# Docs\n\nin docs\n", &query, hit),
        None
    );
    // A pattern that will not compile is nothing to land on either.
    let mut broken = plain("a(");
    broken.regular_expression = true;
    assert_eq!(search::relocate("a(\n", &broken, hit), None);
}

/// **A hit knows where its match starts, in characters of the whole line.**
///
/// M5.11, decision 6: a hit opens at its match, and the editor's `Position`
/// counts characters, not bytes — `ё` and `ж` are two bytes each. And the
/// column is the line's, counted **before the cut**: a row of a minified
/// file draws a window around the match, and the tab it opens holds the
/// whole line.
#[test]
fn a_hit_knows_the_character_column_of_its_first_match_before_the_cut() {
    let long = format!("{}needle\n", "x".repeat(4000));
    let root = tree(
        "column",
        &[
            ("a.rs", "    // ёж needle\n".as_bytes()),
            ("b.js", long.as_bytes()),
        ],
    );

    let hits = found(&root, &plain("needle"));
    assert_eq!(hits.len(), 2);

    assert_eq!(hits[0].path, PathBuf::from("a.rs"));
    assert_eq!(hits[0].column, 10, "characters, not the 12 bytes before it");

    assert_eq!(hits[1].path, PathBuf::from("b.js"));
    assert!(hits[1].cut);
    assert_eq!(hits[1].column, 4000, "the whole line's column");
    assert_eq!(
        hits[1].ranges[0].start,
        search::LINE_LIMIT / 3,
        "while the row's range is the cut's"
    );
}

/// Take what a walk has said until `done` answers true or five seconds
/// pass — **a clock, not a count of looks**: a count is two seconds on an
/// idle machine and much less under `just check`.
fn walk_until(walk: &mut search::Walk, mut done: impl FnMut(&search::Walk) -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        walk.take();
        if done(walk) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    false
}

/// Many files with the needle in each — enough that a walk is still
/// walking when its first files have answered.
fn many(name: &str, count: usize) -> Scratch {
    let files: Vec<(String, Vec<u8>)> = (0..count)
        .map(|at| (format!("d{}/f{at}.txt", at / 100), b"alpha\n".to_vec()))
        .collect();
    let borrowed: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    tree(name, &borrowed)
}

/// **A walk streams: its first files are on hand before it has finished.**
/// M8.9, decision 12 — the engine the Search tool, the Find in Files popup
/// and Search Everywhere's *Text* tab share. M8.7 measured a tree with no
/// ignore file at twelve seconds, and the row a person wanted was usually
/// in the first ten; an engine that answered only at the end would be that
/// long a blank.
#[test]
fn a_walk_streams_before_it_finishes() {
    let root = many("walk-streams", 3000);
    let mut walk = search::Walk::default();
    assert!(!walk.running(), "a walk nobody started is idle");

    walk.restart(&root, plain("alpha"));
    assert!(walk.running());

    let mut seen_while_running = false;
    let finished = walk_until(&mut walk, |walk| {
        if walk.running() && !walk.hits().is_empty() {
            seen_while_running = true;
        }
        !walk.running()
    });

    assert!(finished, "the walk ends");
    assert!(
        seen_while_running,
        "hits were on hand while it still walked"
    );
    assert_eq!(walk.hits().len(), 3000);
    assert_eq!(walk.files(), 3000);
    assert!(!walk.capped());
    assert_eq!(walk.refused(), None);
}

/// **A walk replaced by another says nothing more.** A person typing
/// replaces the query five times a second, and a hit from a query nobody
/// asks any more is a row that cannot be explained.
#[test]
fn a_replaced_walk_says_nothing_more() {
    let root = many("walk-replaced", 2000);
    std::fs::write(root.join("beta.txt"), b"beta\n").expect("a file");
    let mut walk = search::Walk::default();

    walk.restart(&root, plain("alpha"));
    walk.restart(&root, plain("beta"));

    assert!(walk_until(&mut walk, |walk| !walk.running()));
    // Anything the first walk had still in its channel would land here.
    std::thread::sleep(std::time::Duration::from_millis(50));
    walk.take();

    assert_eq!(walk.hits().len(), 1, "{:?}", walk.hits().first());
    assert_eq!(walk.hits()[0].text, "beta");
    assert_eq!(walk.files(), 1);
}

/// **The old hits stay until the new walk answers.** What is drawn is the
/// same answer one narrowing behind; blanking it per keystroke is the
/// blink M8.3.4 took out of the Log. A walk that finds nothing still
/// replaces them when it ends, and a pattern that will not compile says so
/// and leaves them.
#[test]
fn the_old_hits_stay_until_the_new_walk_answers() {
    let root = tree(
        "walk-stays",
        &[
            ("a.txt", b"alpha\n" as &[u8]),
            ("b.txt", b"beta\n" as &[u8]),
        ],
    );
    let mut walk = search::Walk::default();
    walk.restart(&root, plain("alpha"));
    assert!(walk_until(&mut walk, |walk| !walk.running()));
    assert_eq!(walk.hits().len(), 1);

    walk.restart(&root, plain("beta"));
    assert_eq!(walk.hits()[0].text, "alpha", "still on screen as it starts");
    assert!(walk_until(&mut walk, |walk| !walk.running()));
    assert_eq!(walk.hits()[0].text, "beta", "replaced when it answers");

    let mut broken = plain("a(");
    broken.regular_expression = true;
    walk.restart(&root, broken);
    assert!(walk_until(&mut walk, |walk| !walk.running()));
    assert!(walk.refused().is_some(), "the regex crate's words");
    assert_eq!(
        walk.hits()[0].text,
        "beta",
        "a refusal leaves the last answer"
    );

    walk.restart(&root, plain("gamma"));
    assert!(walk_until(&mut walk, |walk| !walk.running()));
    assert!(
        walk.hits().is_empty(),
        "nothing found replaces them at the end"
    );
    assert_eq!(walk.refused(), None);

    walk.restart(&root, plain("beta"));
    assert!(walk_until(&mut walk, |walk| !walk.running()));
    walk.restart(&root, plain(""));
    assert!(walk.hits().is_empty(), "nothing typed is no answer at all");
    assert!(!walk.running());
}
