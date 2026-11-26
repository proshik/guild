//! What a language server says it is doing, kept by token — M8.5.1.
//!
//! Every message here is one a real server sent on 2026-09-17: gopls,
//! ruby-lsp, jdtls, kotlin-lsp and expert, traced while they started.

use guild_desktop::lsp::progress::{Line, Work};
use serde_json::json;

fn begin(work: &mut Work, token: serde_json::Value, value: serde_json::Value) -> bool {
    let mut value = value;
    value["kind"] = json!("begin");
    work.absorb("$/progress", &json!({"token": token, "value": value}))
}

fn end(work: &mut Work, token: serde_json::Value) -> bool {
    work.absorb(
        "$/progress",
        &json!({"token": token, "value": {"kind": "end"}}),
    )
}

/// gopls: a title and a message, and nothing once it ends.
#[test]
fn a_task_is_shown_with_its_message_until_it_ends() {
    let mut work = Work::default();
    assert_eq!(work.headline(), None, "a quiet server says nothing");

    assert!(begin(
        &mut work,
        json!("gopls-1"),
        json!({"title": "Setting up workspace", "message": "Loading packages..."}),
    ));
    assert_eq!(
        work.headline(),
        Some(Line {
            title: "Setting up workspace".into(),
            detail: Some("Loading packages...".into()),
            percentage: None,
            more: 0,
        })
    );

    assert!(end(&mut work, json!("gopls-1")));
    assert_eq!(work.headline(), None);
    assert!(work.is_idle());
}

/// expert: two tasks at once, and the first to end must not take the
/// other with it. This is the bug M8.5's one slot had.
#[test]
fn two_tasks_at_once_are_two_and_ending_one_leaves_the_other() {
    let mut work = Work::default();
    begin(
        &mut work,
        json!(1),
        json!({"title": "[ex] Starting engine node"}),
    );
    begin(
        &mut work,
        json!(2),
        json!({"title": "[ex] Preparing engine"}),
    );

    let line = work.headline().expect("working");
    assert_eq!(
        line.title, "[ex] Preparing engine",
        "the newest is in front"
    );
    assert_eq!(line.more, 1);
    assert_eq!(work.tasks().count(), 2);

    end(&mut work, json!(2));
    let line = work.headline().expect("still working");
    assert_eq!(line.title, "[ex] Starting engine node");
    assert_eq!(line.more, 0);

    end(&mut work, json!(1));
    assert!(work.is_idle());
}

/// A number token and a string token that print alike are still two:
/// the protocol's token is `integer | string`.
#[test]
fn a_number_token_is_not_the_string_of_it() {
    let mut work = Work::default();
    begin(&mut work, json!(7), json!({"title": "number"}));
    begin(&mut work, json!("7"), json!({"title": "string"}));
    assert_eq!(work.tasks().count(), 2);
    end(&mut work, json!(7));
    assert_eq!(work.headline().expect("one left").title, "string");
}

/// ruby-lsp: a percentage at the start and in every report, and a
/// report's message replaces the one before.
#[test]
fn a_report_moves_the_percentage_and_the_message() {
    let mut work = Work::default();
    begin(
        &mut work,
        json!("ruby"),
        json!({"title": "Ruby LSP: indexing files", "message": "0% completed", "percentage": 0}),
    );
    assert_eq!(work.headline().unwrap().percentage, Some(0));

    let changed = work.absorb(
        "$/progress",
        &json!({"token": "ruby", "value": {"kind": "report", "message": "42% completed", "percentage": 42}}),
    );
    assert!(changed);
    let line = work.headline().unwrap();
    assert_eq!(line.percentage, Some(42));
    assert_eq!(line.detail.as_deref(), Some("42% completed"));

    // A report with neither field changes nothing, and says so.
    let changed = work.absorb(
        "$/progress",
        &json!({"token": "ruby", "value": {"kind": "report"}}),
    );
    assert!(!changed);

    // A percentage past a hundred is a server's mistake, not a bar
    // drawn past its end.
    work.absorb(
        "$/progress",
        &json!({"token": "ruby", "value": {"kind": "report", "percentage": 140}}),
    );
    assert_eq!(work.headline().unwrap().percentage, Some(100));
}

/// A report or an end for a token nobody began is dropped.
#[test]
fn a_token_nobody_began_is_ignored() {
    let mut work = Work::default();
    assert!(!work.absorb(
        "$/progress",
        &json!({"token": "x", "value": {"kind": "report", "percentage": 5}}),
    ));
    assert!(!end(&mut work, json!("x")));
    assert!(work.is_idle());
}

/// kotlin-lsp: Gradle's own words are the detail of *Importing*, the
/// last line that says something. When the work is over they go.
#[test]
fn the_import_log_is_the_detail_of_a_task_with_no_message() {
    let mut work = Work::default();
    begin(&mut work, json!("import"), json!({"title": "Importing"}));
    let changed = work.absorb(
        "intellij/importLog",
        &json!({"type": 3, "message": "> Task :prepareKotlinIdeaImport UP-TO-DATE\n", "failed": false}),
    );
    assert!(changed);
    assert_eq!(
        work.headline().unwrap().detail.as_deref(),
        Some("> Task :prepareKotlinIdeaImport UP-TO-DATE")
    );

    // A blank line is not news.
    assert!(!work.absorb("intellij/importLog", &json!({"type": 3, "message": "\n"})));
    assert_eq!(
        work.headline().unwrap().detail.as_deref(),
        Some("> Task :prepareKotlinIdeaImport UP-TO-DATE")
    );

    end(&mut work, json!("import"));
    assert!(work.is_idle(), "the log does not outlive the work");
    assert_eq!(work.headline(), None);
}

/// A task's own message beats the log.
#[test]
fn a_message_of_the_task_wins_over_the_log() {
    let mut work = Work::default();
    begin(
        &mut work,
        json!(1),
        json!({"title": "Indexing", "message": "12 files"}),
    );
    work.absorb(
        "intellij/importLog",
        &json!({"message": "BUILD SUCCESSFUL in 827ms\n"}),
    );
    assert_eq!(work.headline().unwrap().detail.as_deref(), Some("12 files"));
}

/// jdtls: *Starting* is said before any task has begun, and it is the
/// title while nothing else is; *Started* and *ServiceReady* end it.
#[test]
fn a_java_server_starting_is_said_until_it_has_started() {
    let mut work = Work::default();
    assert!(work.absorb(
        "language/status",
        &json!({"type": "Starting", "message": "0% Starting Java Language Server"}),
    ));
    assert_eq!(
        work.headline(),
        Some(Line {
            title: "0% Starting Java Language Server".into(),
            detail: None,
            percentage: None,
            more: 0,
        })
    );

    // A task in front wins over the status.
    begin(
        &mut work,
        json!(1),
        json!({"title": "Initialize Workspace"}),
    );
    assert_eq!(work.headline().unwrap().title, "Initialize Workspace");
    end(&mut work, json!(1));

    // Ending the last task ends what the status said, too.
    assert!(work.is_idle());

    work.absorb(
        "language/status",
        &json!({"type": "Starting", "message": "100% Starting Java Language Server - Opening 'fx'."}),
    );
    assert!(work.absorb(
        "language/status",
        &json!({"type": "ServiceReady", "message": "ServiceReady"}),
    ));
    assert!(work.is_idle());
    // `ProjectStatus` says nothing about starting and changes nothing.
    assert!(!work.absorb(
        "language/status",
        &json!({"type": "ProjectStatus", "message": "OK"}),
    ));
}

/// Anything else is not ours to read.
#[test]
fn other_notifications_change_nothing() {
    let mut work = Work::default();
    assert!(!work.absorb("language/eventNotification", &json!({"eventType": 200})));
    assert!(!work.absorb("$/progress", &json!("nonsense")));
    assert!(work.is_idle());
}

/// A fault names the command, because the command is what a person
/// installs or looks up, and it says which of three things happened.
#[test]
fn a_fault_is_said_with_the_command_it_is_about() {
    use guild_desktop::i18n::{Language, Ui};
    use guild_desktop::lsp::progress::Fault;

    let english = Ui(Language::English);
    assert_eq!(
        english.server_fault("gopls", &Fault::Missing),
        "gopls not found"
    );
    assert_eq!(
        english.server_fault("gopls", &Fault::Dying),
        "gopls keeps stopping"
    );
    assert_eq!(
        english.server_fault("gopls", &Fault::Refused("no go.mod".into())),
        "gopls refused to start: no go.mod"
    );

    // Not started, and the files that would start it named: what a
    // person adds is one of them.
    assert_eq!(
        english.server_fault(
            "sourcekit-lsp",
            &Fault::Unprepared(&["Package.swift", "buildServer.json"])
        ),
        "sourcekit-lsp needs Package.swift or buildServer.json"
    );

    let russian = Ui(Language::Russian);
    for fault in [
        Fault::Missing,
        Fault::Dying,
        Fault::Refused("x".into()),
        Fault::Unprepared(&["Package.swift"]),
    ] {
        let said = russian.server_fault("gopls", &fault);
        assert!(said.contains("gopls"), "{said}");
        assert_ne!(said, english.server_fault("gopls", &fault));
    }
}
