//! What `git log` says in the marked format, and what `git show
//! --name-status` says of one commit — parsed on captured output, never
//! a repository. The fixtures come from a scratch repository built for
//! them: five commits, a merge among them, a rename in the last, a body
//! with a blank line and a `#` line, and an author with a non-ASCII name.

use std::path::Path;

use guild_desktop::git::log::{files_of, parse, refs_label};
use guild_desktop::git::{Filter, Scope, log_args};
use guild_desktop::i18n::{Language, Ui};
use guild_desktop::theme::Mark;

const LOG: &[u8] = include_bytes!("../fixtures/log.bin");
const SHOW: &[u8] = include_bytes!("../fixtures/show-name-status.bin");
/// The same capture in the seven-field shape the format had before
/// M8.3.4, kept so the parser can be asked to refuse it.
const SEVEN: &[u8] = include_bytes!("../fixtures/log-seven-fields.bin");

/// Every commit of the capture, newest first, with its parents: one for
/// most, two for the merge, none for the first.
#[test]
fn the_log_parses_newest_first_with_its_parents() {
    let commits = parse(LOG).expect("the marked format");
    assert_eq!(commits.len(), 5);
    let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(
        summaries,
        [
            "Rename old to new, and touch three files",
            "Merge the feature branch",
            "Add b on the trunk",
            "Change a on the branch",
            "First commit",
        ]
    );
    assert_eq!(commits[0].parents.len(), 1);
    assert_eq!(commits[1].parents.len(), 2, "a merge has two parents");
    assert!(commits[4].parents.is_empty(), "the first commit has none");
    assert!(commits.iter().all(|c| c.sha.len() == 40));
    assert!(commits.iter().all(|c| c.time > 1_700_000_000));
    assert!(commits[0].time >= commits[4].time);
}

/// The author's name as written, the body with its blank line and its
/// `#` line intact — which is what the marked format is for.
#[test]
fn the_author_and_the_body_arrive_as_written() {
    let commits = parse(LOG).expect("the marked format");
    let last = &commits[0];
    assert_eq!(last.author, "Иван Петров");
    assert_eq!(last.email, "ivan@example.invalid");
    assert_eq!(
        last.body,
        "The body has a blank line above it and a line that starts with a hash:\n#42 is the ticket, not a comment."
    );
    assert_eq!(commits[1].author, "Guild");
    assert_eq!(commits[1].body, "", "a one-line message has no body");
}

/// Bytes that are not the format are an error, not an empty log.
#[test]
fn what_is_not_the_format_is_an_error() {
    assert!(parse(b"not a log at all").is_err());
    assert!(parse(b"").expect("an empty log").is_empty());
}

/// One commit's files with their marks: modified, added, deleted, and
/// a rename as one file that says where it came from.
#[test]
fn a_commits_files_come_with_marks_and_a_rename_says_where_from() {
    let files = files_of(SHOW).expect("name-status");
    let listed: Vec<(&Path, Mark)> = files.iter().map(|f| (f.path.as_path(), f.mark)).collect();
    assert_eq!(
        listed,
        [
            (Path::new("a.txt"), Mark::Modified),
            (Path::new("c.txt"), Mark::Added),
            (Path::new("gone.txt"), Mark::Deleted),
            (Path::new("src/new.rs"), Mark::Modified),
        ]
    );
    assert_eq!(files[3].from.as_deref(), Some(Path::new("src/old.rs")));
    assert!(files[..3].iter().all(|f| f.from.is_none()));
}

/// How long ago, in words a person reads at a glance: the last minute,
/// then minutes, hours, yesterday, days — and a date once a week is past,
/// because "9 days ago" makes a person count.
#[test]
fn a_time_reads_relative_to_now_and_as_a_date_past_a_week() {
    let ui = Ui(Language::English);
    let now = 1_788_000_000; // 2026-08-29T15:33:20Z
    assert_eq!(ui.relative_time(now - 20, now), "just now");
    assert_eq!(ui.relative_time(now - 3 * 60, now), "3 minutes ago");
    assert_eq!(ui.relative_time(now - 60, now), "1 minute ago");
    assert_eq!(ui.relative_time(now - 5 * 3600, now), "5 hours ago");
    assert_eq!(ui.relative_time(now - 30 * 3600, now), "yesterday");
    assert_eq!(ui.relative_time(now - 3 * 86_400, now), "3 days ago");
    assert_eq!(ui.relative_time(now - 9 * 86_400, now), "20 Aug 2026");
    assert_eq!(
        ui.relative_time(now + 3600, now),
        "just now",
        "a clock ahead of ours"
    );

    let ru = Ui(Language::Russian);
    for ago in [20, 60, 180, 5 * 3600, 30 * 3600, 3 * 86_400, 9 * 86_400] {
        assert_ne!(
            ru.relative_time(now - ago, now),
            ui.relative_time(now - ago, now)
        );
        assert!(!ru.relative_time(now - ago, now).is_empty());
    }
    assert_eq!(ru.relative_time(now - 9 * 86_400, now), "20 авг 2026");
}

/// A log asked for a named branch puts the name after the options and
/// follows it with `--`: `git log` takes `[<options>] [<revision-range>]
/// [[--] <path>…]`, and without the separator a branch that shares its
/// name with a file is ambiguous — git refuses rather than guessing.
#[test]
fn a_named_branch_goes_after_the_options_and_before_a_separator() {
    let plain = guild_desktop::git::log_args(&Scope::Head, &Filter::default(), 0, 200);
    assert_eq!(plain.first().map(String::as_str), Some("log"));
    assert!(plain.contains(&"-z".to_owned()));
    assert_eq!(plain.last().map(String::as_str), Some("0"), "the skip");
    assert!(
        !plain.contains(&"--".to_owned()),
        "nothing to separate when no ref is named"
    );

    let named = guild_desktop::git::log_args(
        &Scope::Named("m8-3-2-menus".to_owned()),
        &Filter::default(),
        400,
        200,
    );
    let tail: Vec<&str> = named
        .iter()
        .rev()
        .take(4)
        .rev()
        .map(String::as_str)
        .collect();
    assert_eq!(tail, ["--skip", "400", "m8-3-2-menus", "--"]);
}

/// **The date column.** Under an hour it is still relative, because
/// that is what a person is reading the top of the log for; today and
/// yesterday get a clock, because "3 hours ago" and "5 hours ago" are the
/// same glance and `10:00` is not; older is a date and nothing else.
#[test]
fn a_commit_date_reads_as_the_log_column_says_it() {
    let ui = Ui(Language::English);
    // 2026-09-04 15:00 UTC.
    let now = 1_788_534_000;

    assert_eq!(ui.commit_date(now - 60, now, 0), "1 minute ago");
    assert_eq!(ui.commit_date(now - 21 * 60, now, 0), "21 minutes ago");
    assert_eq!(ui.commit_date(now - 5 * 3600, now, 0), "Today 10:00");
    assert_eq!(ui.commit_date(now - 20 * 3600, now, 0), "Yesterday 19:00");
    assert_eq!(ui.commit_date(now - 40 * 86_400, now, 0), "26.07.2026");
}

/// The clock is the window's, not UTC's: a column that said UTC would put
/// half the world's commits at the wrong hour.
#[test]
fn the_clock_is_local() {
    let ui = Ui(Language::English);
    let now = 1_788_534_000;

    // Three hours east: the same instant is three hours later on the wall.
    assert_eq!(ui.commit_date(now - 5 * 3600, now, 3 * 3600), "Today 13:00");
}

/// **The trap the offset is a parameter for.** A commit made at half past
/// eleven UTC on the third is *today* for anyone three hours east, and
/// *yesterday* for anyone at UTC — the same commit, the same instant, two
/// right answers. A formatter that looked the offset up for itself could
/// not be asked either question.
#[test]
fn midnight_is_the_local_midnight() {
    let ui = Ui(Language::English);
    // 2026-09-04 01:50 UTC; the commit is 2026-09-03 23:30 UTC.
    let now = 1_788_486_600;
    let at = now - 8_400;

    assert_eq!(ui.commit_date(at, now, 0), "Yesterday 23:30");
    assert_eq!(ui.commit_date(at, now, 3 * 3600), "Today 02:30");
}

/// Both languages, because a date column is read more often than any
/// sentence in the window. The digits are the same in both: a month's
/// name in a column is three characters that never line up.
#[test]
fn the_date_column_speaks_both_languages() {
    let ui = Ui(Language::Russian);
    let now = 1_788_534_000;

    assert_eq!(ui.commit_date(now - 21 * 60, now, 0), "21 минуту назад");
    assert_eq!(ui.commit_date(now - 5 * 3600, now, 0), "Сегодня 10:00");
    assert_eq!(ui.commit_date(now - 20 * 3600, now, 0), "Вчера 19:00");
    assert_eq!(ui.commit_date(now - 40 * 86_400, now, 0), "26.07.2026");
}

/// **`%D` gives the refs, and each shape is a name a person reads.**
/// git prints `HEAD -> main` for the branch `HEAD` is on, a bare name for
/// any other, and `tag: v1.0` for a tag. The arrow and the word `tag:`
/// are git's punctuation, not part of the name, and the row shows names.
#[test]
fn a_commit_carries_the_refs_pointing_at_it() {
    let commits = parse(LOG).expect("the marked format with refs");

    assert_eq!(commits[0].refs, ["main"], "HEAD -> main is the branch main");
    assert_eq!(
        commits[3].refs,
        ["side", "feature"],
        "two branches, in git's order"
    );
    assert_eq!(commits[4].refs, ["v1.0"], "tag: v1.0 is the tag v1.0");
}

/// A commit with nothing pointing at it — which is almost every commit —
/// carries no refs rather than one empty name.
#[test]
fn a_commit_with_no_refs_carries_none() {
    let commits = parse(LOG).expect("the marked format with refs");

    assert!(
        commits[1].refs.is_empty(),
        "the merge has nothing pointing at it"
    );
    assert!(commits[2].refs.is_empty());
    assert!(commits.iter().all(|c| c.refs.iter().all(|r| !r.is_empty())));
}

/// **A capture in the old format must fail, not shift.** The field count
/// went from seven to eight; a parser that took what it was given would
/// read the summary as the refs and the body as the summary, and every
/// row in the window would be quietly wrong.
#[test]
fn a_capture_without_the_refs_field_is_an_error() {
    assert!(
        parse(SEVEN).is_err(),
        "the seven-field capture is no longer the format"
    );
}

/// **An empty filter adds no options.** Everything below adds
/// arguments; nothing changes the shape of a plain page, and this is the
/// test that says so.
#[test]
fn no_filter_asks_what_it_always_asked() {
    let plain = log_args(&Scope::Head, &Filter::default(), 0, 200);

    assert!(!plain.iter().any(|arg| arg.starts_with("--grep")));
    assert!(!plain.iter().any(|arg| arg.starts_with("--author")));
    assert!(!plain.iter().any(|arg| arg.starts_with("--since")));
    assert_eq!(plain.first().map(String::as_str), Some("log"));
}

/// **The three scopes are three different asks**, which is the whole
/// reason the scope is not an `Option<String>`: `All` is every ref,
/// `Head` is the head of the branch checked out — git's own default and
/// so no argument at all — and `Named` is one branch. While `All` and
/// `Head` were one value the cross on *Branch: HEAD* had nothing to take
/// off, and pressing it changed nothing on the screen.
#[test]
fn every_branch_the_head_and_one_branch_are_three_asks() {
    let all = log_args(&Scope::All, &Filter::default(), 0, 200);
    let head = log_args(&Scope::Head, &Filter::default(), 0, 200);
    let named = log_args(&Scope::Named("main".to_owned()), &Filter::default(), 0, 200);

    assert!(all.contains(&"--all".to_owned()), "{all:?}");
    assert!(!all.contains(&"--".to_owned()), "an option, not a revision");

    assert!(!head.contains(&"--all".to_owned()), "{head:?}");
    assert_ne!(all, head, "the default and the current branch differ");

    assert!(!named.contains(&"--all".to_owned()), "{named:?}");
    assert_eq!(named.last().map(String::as_str), Some("--"));

    assert_eq!(
        Scope::default(),
        Scope::All,
        "the log starts on every branch"
    );
    assert_eq!(Scope::Named("main".to_owned()).branch(), Some("main"));
    assert_eq!(Scope::Head.branch(), None);
}

/// **The scope survives a filter.** `--all` is an option and the other
/// options are options: putting a narrowing on does not take the scope
/// off, which is what a person who typed a word into a log of every
/// branch expects to still be reading.
#[test]
fn every_branch_keeps_its_option_under_a_filter() {
    let args = log_args(&Scope::All, &Filter::text("hello"), 0, 200);

    assert!(args.contains(&"--all".to_owned()), "{args:?}");
    assert!(args.contains(&"--grep=hello".to_owned()));
}

/// Words search the message, case-insensitively: a person types what they
/// remember writing, not what they remember capitalising.
#[test]
fn words_become_a_grep() {
    let args = log_args(&Scope::Head, &Filter::text("branch button"), 0, 200);

    assert!(
        args.contains(&"--grep=branch button".to_owned()),
        "{args:?}"
    );
    assert!(args.contains(&"--regexp-ignore-case".to_owned()));
}

/// **A hex prefix means *that commit*, not a text search** — decision 4.
/// Pasting a hash and getting the commit is what the gesture is for, and
/// `--grep=8f607b1` would find only a message that mentions it.
#[test]
fn a_hex_prefix_becomes_the_revision() {
    let args = log_args(&Scope::Head, &Filter::text("8f607b1"), 0, 200);

    assert!(
        !args.iter().any(|arg| arg.starts_with("--grep")),
        "{args:?}"
    );
    assert!(args.contains(&"8f607b1".to_owned()));
    assert_eq!(args.last().map(String::as_str), Some("--"), "and separated");
}

/// Three characters are a word, not a hash: `abc` is something a person
/// wrote, and a hash that short matches half the repository.
#[test]
fn three_hex_characters_are_still_words() {
    let args = log_args(&Scope::Head, &Filter::text("abc"), 0, 200);

    assert!(args.contains(&"--grep=abc".to_owned()), "{args:?}");
}

/// A word made of hex letters is still a word once it is not a plausible
/// hash — `added` is five letters, all of them hex digits.
#[test]
fn a_word_that_looks_like_hex_but_is_not_a_hash() {
    let args = log_args(&Scope::Head, &Filter::text("deadbeef cafe"), 0, 200);

    assert!(
        args.contains(&"--grep=deadbeef cafe".to_owned()),
        "a space means words, {args:?}"
    );
}

/// The author and the date, as git spells them.
#[test]
fn an_author_and_a_date_become_their_own_options() {
    let filter = Filter {
        author: Some("Sam".to_owned()),
        since: Some("1.week".to_owned()),
        ..Filter::default()
    };
    let args = log_args(&Scope::Head, &filter, 0, 200);

    assert!(args.contains(&"--author=Sam".to_owned()), "{args:?}");
    assert!(args.contains(&"--since=1.week".to_owned()));
}

/// **A branch and a hash cannot both be the revision.** A person who has
/// picked a branch and then pasted a hash means the hash: it names one
/// commit and the branch names a range, and the narrower answer is the
/// one they just typed.
#[test]
fn a_hash_wins_over_a_chosen_branch() {
    let args = log_args(
        &Scope::Named("m8-3-4-git-tool".to_owned()),
        &Filter::text("8f607b1"),
        0,
        200,
    );

    assert!(args.contains(&"8f607b1".to_owned()), "{args:?}");
    assert!(!args.contains(&"m8-3-4-git-tool".to_owned()));
}

/// The named ref still goes after the options and before the separator,
/// which is the rule `git log` has and M8.3 wrote down.
#[test]
fn a_branch_with_a_filter_keeps_its_place() {
    let args = log_args(
        &Scope::Named("main".to_owned()),
        &Filter::text("hello"),
        0,
        200,
    );
    let branch = args.iter().position(|arg| arg == "main").expect("the ref");
    let grep = args
        .iter()
        .position(|arg| arg.starts_with("--grep"))
        .expect("the grep");

    assert!(grep < branch, "options first: {args:?}");
    assert_eq!(args.get(branch + 1).map(String::as_str), Some("--"));
}

/// **The refs must not push the summary off the row.** They are the
/// least of what a row says and they were taking whatever they liked:
/// two long branch names covered the message entirely.
///
/// The rule: several names join with `&`, and a name too
/// long for the room loses its leading path segments to `..` before it
/// loses its tail.
#[test]
fn several_refs_join_with_an_ampersand() {
    assert_eq!(
        refs_label(&names(&["origin", "master"]), 40),
        "origin & master"
    );
    assert_eq!(refs_label(&names(&["main"]), 40), "main");
    assert_eq!(refs_label(&[], 40), "");
}

/// A path-shaped name gives up its prefix first: `fix/` says nothing that
/// `e2e-web-qa-delegation` does not.
#[test]
fn a_long_name_loses_its_prefix_before_its_tail() {
    let long = names(&["fix/e2e-web-qa-delegation-and-token-refresh"]);

    assert_eq!(
        refs_label(&long, 30),
        "../e2e-web-qa-delegation-and-\u{2026}"
    );
}

/// With nothing left to drop it is truncated, because a row has a width
/// and a name may not.
#[test]
fn a_name_with_no_prefix_is_truncated() {
    let long = names(&["averyverylongbranchnamewithnoslashesatall"]);

    assert_eq!(refs_label(&long, 12), "averyverylo\u{2026}");
}

/// What fits is left exactly as git printed it.
#[test]
fn a_name_that_fits_is_untouched() {
    assert_eq!(
        refs_label(&names(&["m8-3-4-git-tool"]), 30),
        "m8-3-4-git-tool"
    );
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}
