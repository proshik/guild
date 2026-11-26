//! What the servers said, as the Problems panel reads it. M8.8.5.
//!
//! The model is pure — a list of problems in, rows out — so all of it is
//! checked here, without a window and without a server.

use std::collections::HashSet;
use std::path::PathBuf;

use guild_desktop::i18n::{Language, Ui};
use guild_desktop::root::problems::{
    CAP, Problem, Quiet, Row, Severity, counts, label_of, of, quiet, settle,
};

/// One problem, with the fields a test does not care about filled in.
fn problem(path: &str, line: u32, severity: Severity) -> Problem {
    Problem {
        path: PathBuf::from(path),
        line,
        character: 0,
        severity,
        message: format!("something at {line}"),
        source: None,
    }
}

/// A chain of directories with one child each is one row, as it is in
/// the Log's opened commit and in the search results — the rows a person
/// walks past to reach a file are the ones worth removing.
#[test]
fn a_run_of_single_child_directories_is_one_row() {
    let problems = vec![
        problem("crates/desktop/src/lib.rs", 4, Severity::Error),
        problem("crates/desktop/src/root/lsp.rs", 9, Severity::Warning),
    ];

    let rows = of(&problems, &HashSet::new());

    let directories: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Directory { label, .. } => Some(label.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        directories,
        ["crates/desktop/src", "root"],
        "the chain down to the first fork is one row: {rows:#?}"
    );
}

/// A directory says how many problems are beneath it and wears the worst
/// of them, because a count alone cannot tell six hints from six errors.
#[test]
fn a_directory_carries_the_count_and_the_worst_beneath_it() {
    let problems = vec![
        problem("src/one.rs", 1, Severity::Hint),
        problem("src/two.rs", 2, Severity::Error),
        problem("src/two.rs", 7, Severity::Warning),
    ];

    let rows = of(&problems, &HashSet::new());

    let Some(Row::Directory {
        problems: n, worst, ..
    }) = rows.first()
    else {
        panic!("a directory row first: {rows:#?}");
    };
    assert_eq!(*n, 3);
    assert_eq!(*worst, Severity::Error);
}

/// A folded directory takes everything under it; a folded file keeps its
/// own row and loses its problems — so there is always something left to
/// press.
#[test]
fn folding_a_directory_hides_it_and_folding_a_file_does_not() {
    let problems = vec![
        problem("src/one.rs", 1, Severity::Error),
        problem("lib/two.rs", 2, Severity::Error),
    ];

    let shut: HashSet<PathBuf> = HashSet::from([PathBuf::from("src")]);
    let rows = of(&problems, &shut);
    assert_eq!(
        rows.iter()
            .filter(|row| matches!(row, Row::File { .. } | Row::Problem { .. }))
            .count(),
        2,
        "only the other directory's file and its problem: {rows:#?}"
    );

    let shut: HashSet<PathBuf> = HashSet::from([PathBuf::from("src/one.rs")]);
    let rows = of(&problems, &shut);
    assert_eq!(
        rows.iter()
            .filter(|row| matches!(row, Row::File { .. }))
            .count(),
        2,
        "both files still have a row: {rows:#?}"
    );
    assert_eq!(
        rows.iter()
            .filter(|row| matches!(row, Row::Problem { .. }))
            .count(),
        1,
        "and the folded one lost its problem: {rows:#?}"
    );
}

/// A problem row points back into the list it was built from by index,
/// not by carrying the problem: the rows are rebuilt whenever a server
/// publishes, and a row that borrowed one would tie its lifetime to a
/// list that is replaced whole.
#[test]
fn a_problem_row_is_an_index_into_the_list() {
    let problems = vec![
        problem("a.rs", 1, Severity::Error),
        problem("b.rs", 2, Severity::Error),
    ];

    let rows = of(&problems, &HashSet::new());

    let indexes: Vec<usize> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Problem { at, .. } => Some(*at),
            _ => None,
        })
        .collect();
    assert_eq!(indexes, [0, 1]);
}

/// The button in the status bar counts errors and warnings, and nothing
/// else. Information and hints are worth a row and not a number: a bar
/// saying *84* over a clean project would be counting rust-analyzer's
/// unused-import hints and calling them problems.
#[test]
fn the_counts_are_errors_and_warnings_only() {
    let problems = vec![
        problem("a.rs", 1, Severity::Error),
        problem("a.rs", 2, Severity::Warning),
        problem("a.rs", 3, Severity::Warning),
        problem("a.rs", 4, Severity::Info),
        problem("a.rs", 5, Severity::Hint),
    ];

    assert_eq!(counts(&problems), (1, 2));
}

/// A server may publish a diagnostic with no severity at all — the
/// protocol allows it and says the client decides. **Information**, not
/// error: a mistake invented out of a missing field would be a squiggle
/// nobody can explain.
#[test]
fn a_diagnostic_with_no_severity_is_information() {
    assert_eq!(Severity::of(None), Severity::Info);
    assert_eq!(
        Severity::of(Some(lsp_types::DiagnosticSeverity::ERROR)),
        Severity::Error
    );
    assert_eq!(
        Severity::of(Some(lsp_types::DiagnosticSeverity::HINT)),
        Severity::Hint
    );
}

/// The list is sorted into reading order — by file, then down the file —
/// because a `HashMap` of servers' answers has no order at all, and a
/// panel whose rows move between frames is a panel nobody can click.
#[test]
fn the_list_is_sorted_into_reading_order() {
    let problems = vec![
        problem("b.rs", 9, Severity::Error),
        problem("a.rs", 40, Severity::Error),
        problem("a.rs", 4, Severity::Error),
    ];

    let (settled, capped) = settle(problems);

    let order: Vec<(&str, u32)> = settled
        .iter()
        .map(|one| (one.path.to_str().expect("utf-8"), one.line))
        .collect();
    assert_eq!(order, [("a.rs", 4), ("a.rs", 40), ("b.rs", 9)]);
    assert!(!capped);
}

/// **The cap is a constant nobody sees until a project earns it.** Task
/// 0 could not measure a real number — the probe found a tree that
/// compiles — so the tool takes one regardless, on M8.7's reasoning: the
/// cost of a cap that is never reached is a constant, and the cost of
/// not having one is a frame that never ends.
#[test]
fn the_list_stops_at_the_cap_and_says_so() {
    let problems: Vec<Problem> = (0..CAP as u32 + 10)
        .map(|line| problem("a.rs", line, Severity::Warning))
        .collect();

    let (settled, capped) = settle(problems);

    assert_eq!(settled.len(), CAP);
    assert!(capped, "a count with no note is a lie about the project");
}

/// **An empty panel has three reasons and they are three different
/// sentences.** *Nothing is wrong here* is a claim about the project;
/// *nobody has looked* is a claim about us, and saying the first when the
/// second is true is the window telling a person their code is fine when
/// nothing has read it. It is worth a state of its own because on a cold
/// start those two are two minutes apart — the figure M8.5 measured.
#[test]
fn an_empty_panel_says_which_emptiness_it_is() {
    assert_eq!(quiet(0, 0), Quiet::NobodyLooked);
    assert_eq!(quiet(2, 1), Quiet::Looking);
    assert_eq!(quiet(2, 0), Quiet::Clean);

    // A server that has not answered yet is still looking, even where it
    // is the only one there is.
    assert_eq!(quiet(1, 1), Quiet::Looking);
}

/// And the three sentences are three sentences. The state is only worth
/// having if the words differ: a mapping that sent two of them to one
/// label would pass every assertion above and say nothing new on screen.
#[test]
fn the_three_emptinesses_are_worded_differently() {
    for language in Language::ALL {
        let ui = Ui(*language);
        let said: Vec<&str> = [Quiet::NobodyLooked, Quiet::Looking, Quiet::Clean]
            .into_iter()
            .map(|quiet| ui.text(label_of(quiet)))
            .collect();

        let mut unique = said.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 3, "three states, three notes: {said:?}");
    }
}
