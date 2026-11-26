//! The Git tool's Console: what is kept, and how a line reads.

use guild_desktop::console::{Console, LINES, Line};

fn line(at: i64, argv: &[&str], said: &str, failed: bool) -> Line {
    Line {
        at,
        root: "/Users/person/work/guild".to_owned(),
        argv: argv.iter().map(|word| (*word).to_owned()).collect(),
        said: said.to_owned(),
        failed,
    }
}

/// **Down to the milliseconds** — which are what tell two
/// runs of the same command apart, and a switch back and forth is exactly
/// two runs of the same command.
#[test]
fn a_heading_says_when_where_and_what() {
    // 2026-09-04 13:14:21.049 local.
    let at = 1_788_527_661_049;

    assert_eq!(
        line(at, &["switch", "main"], "", false).heading(0),
        "13:14:21.049: [/Users/person/work/guild] git switch main"
    );
}

/// The clock is the window's, as the log's date column is.
#[test]
fn the_heading_is_on_the_local_clock() {
    let at = 1_788_527_661_049;
    let heading = line(at, &["switch", "main"], "", false).heading(3 * 3600);

    assert!(heading.starts_with("16:14:21.049"), "{heading}");
}

/// An argument with a space is one argument, and it is shown as git was
/// given it rather than as a shell would need it quoted — this is a
/// record of what ran, not something to paste back.
#[test]
fn the_argv_is_shown_as_git_was_given_it() {
    let heading = line(0, &["commit", "-F", "/tmp/a b/msg"], "", false).heading(0);

    assert!(heading.ends_with("git commit -F /tmp/a b/msg"), "{heading}");
}

/// **The lid.** A console that grew without one would hold a session's
/// whole history for the sake of a scroll nobody reaches the top of.
#[test]
fn the_oldest_line_goes_when_the_lid_is_reached() {
    let mut console = Console::default();
    for n in 0..LINES + 5 {
        console.push(line(n as i64, &["switch", "main"], "", false));
    }

    let kept: Vec<i64> = console.lines().map(|line| line.at).collect();
    assert_eq!(kept.len(), LINES);
    assert_eq!(kept[0], 5, "the first five went");
    assert_eq!(kept[LINES - 1], (LINES + 4) as i64, "the newest stayed");
}

/// Oldest first: a console is read downwards, and the newest is at the
/// bottom where the eye already is.
#[test]
fn the_lines_come_back_oldest_first() {
    let mut console = Console::default();
    assert!(console.is_empty());
    console.push(line(
        1,
        &["switch", "main"],
        "Switched to branch 'main'",
        false,
    ));
    console.push(line(
        2,
        &["branch", "-d", "spike"],
        "error: not merged",
        true,
    ));

    let lines: Vec<&Line> = console.lines().collect();
    assert_eq!(lines[0].at, 1);
    assert_eq!(lines[1].at, 2);
    assert!(lines[1].failed, "a refusal is marked and keeps its words");
    assert_eq!(lines[1].said, "error: not merged");
}

/// **What the Console holds is one text**, so a selection can cross
/// lines — 2026-09-23, when the person could not copy what git said. A
/// heading per command, what git said under it, and a blank line between
/// two commands; a command git said nothing to is its heading alone.
#[test]
fn the_console_is_one_text_of_headings_and_answers() {
    let at = 1_788_527_661_049;
    let mut console = Console::default();
    console.push(line(at, &["add", "a.rs"], "", false));
    console.push(line(
        at,
        &["commit", "-F", "-"],
        "[main 1b5a743] a message\n",
        false,
    ));

    assert_eq!(
        console.text(0),
        "13:14:21.049: [/Users/person/work/guild] git add a.rs\n\
         \n\
         13:14:21.049: [/Users/person/work/guild] git commit -F -\n\
         [main 1b5a743] a message"
    );
}
