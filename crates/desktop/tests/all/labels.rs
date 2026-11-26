//! Every label, in every language.
//!
//! Out here rather than beside the code: every one goes in through the
//! public door, which makes them a check on that door as well as on what
//! is behind it.

use guild_desktop::i18n::*;
use guild_session::Usage;

/// An empty label is a hole in the interface that users are the first
/// to notice.
#[test]
fn no_language_has_an_empty_string() {
    for language in Language::ALL {
        let ui = Ui(*language);
        for text in Text::ALL {
            assert!(
                !ui.text(*text).trim().is_empty(),
                "empty label {text:?} in language {}",
                language.endonym()
            );
        }
    }
}

/// A translation identical to the original word for word usually
/// means a forgotten string. There are exceptions — proper nouns — but
/// they are few and named explicitly.
///
/// **Two of the three were found the moment the list stopped being kept
/// by hand.** `ToolGit` is *Git* and `LogHead` is *HEAD* in both
/// languages, and both had been outside the hand-written list since they
/// were written — untranslated on purpose, unchecked by accident.
#[test]
fn translations_actually_differ_from_the_original() {
    let same: Vec<&Text> = Text::ALL
        .iter()
        .filter(|text| Ui(Language::English).text(**text) == Ui(Language::Russian).text(**text))
        // The proper nouns. `Git` is the program's name and `HEAD` is
        // git's own word for the place, printed by git and typed by a
        // person into a terminal — translating either would be inventing a
        // second name for something that has one. *Claude Code*, *OpenCode*
        // and *Codex* are products' names, the headings of the counter's
        // list since M12.2.
        .filter(|text| {
            !matches!(
                text,
                Text::AgentClaudeCode
                    | Text::ToolGit
                    | Text::LogHead
                    | Text::AgentOpenCode
                    | Text::AgentCodex
            )
        })
        .collect();

    assert!(same.is_empty(), "labels are not translated: {same:?}");
}

/// A language names itself: the list is read by someone looking for
/// their own.
#[test]
fn languages_are_named_in_their_own_tongue() {
    assert_eq!(Language::English.endonym(), "English");
    assert_eq!(Language::Russian.endonym(), "Русский");
}

/// Cycling must come back to the start, otherwise the last language
/// becomes a trap: you pick it and cannot switch back.
#[test]
fn cycling_through_languages_comes_back_around() {
    let mut language = Language::default();
    for _ in 0..Language::ALL.len() {
        language = language.next();
    }

    assert_eq!(language, Language::default());
}

#[test]
fn english_is_the_default() {
    assert_eq!(Language::default(), Language::English);
}

/// Strings with numbers are assembled by each language in its own
/// way: the word order differs, and one concatenation does not cover
/// it.
#[test]
fn counted_strings_are_built_per_language() {
    assert_eq!(Ui(Language::English).project_sessions(3), "3 sessions");
    assert_eq!(Ui(Language::Russian).project_sessions(3), "сессий 3");
}

/// A notice that carries a path or a reason must keep it. "Could not
/// create the directory" without saying which one, or why, sends a
/// person back to the log — which is the thing notices exist to avoid.
#[test]
fn a_notice_keeps_the_detail_it_was_given() {
    for language in Language::ALL {
        let ui = Ui(*language);

        let directory = ui.notice_not_created("/tmp/guild-x", "permission denied");
        assert!(directory.contains("/tmp/guild-x"), "{directory}");
        assert!(directory.contains("permission denied"), "{directory}");

        // The tree's three, M8.8.3. A rename and a delete can lose work,
        // so the file has to be named in the sentence about it.
        let renamed = ui.notice_not_renamed("tree.rs", "file exists");
        assert!(renamed.contains("tree.rs"), "{renamed}");
        assert!(renamed.contains("file exists"), "{renamed}");

        // The Trash refusal carries AppKit's own sentence and nothing
        // else: it already names the file, in the person's own language.
        let trashed = ui.notice_not_trashed("The file “a.rs” doesn’t exist.");
        assert!(trashed.contains("a.rs"), "{trashed}");

        // The sheet that asks first names it in its question.
        let asked = ui.delete_title("a.rs");
        assert!(asked.contains("a.rs"), "{asked}");
    }
}

/// A diff tab says its base after the file's name, in the language,
/// except the default one: *lib.rs* alone is the last commit against
/// the working tree, as M8.2 named it.
#[test]
fn a_diff_tab_names_its_base_after_the_file() {
    use guild_session::Base;
    for language in Language::ALL {
        let ui = Ui(*language);
        assert_eq!(ui.diff_tab_title("lib.rs", &Base::Head), "lib.rs");
        let staged = ui.diff_tab_title("lib.rs", &Base::Index);
        let unstaged = ui.diff_tab_title("lib.rs", &Base::Unstaged);
        assert!(staged.starts_with("lib.rs · "), "{staged}");
        assert!(unstaged.starts_with("lib.rs · "), "{unstaged}");
        assert_ne!(staged, unstaged);
        assert_eq!(
            ui.diff_tab_title("lib.rs", &Base::Commit("a1b2c3d4e5f6".into())),
            "lib.rs · a1b2c3d"
        );
    }
    assert_eq!(
        Ui(Language::English).diff_tab_title("lib.rs", &Base::Index),
        "lib.rs · staged"
    );
}

/// A commit's notices carry what a person looks for: the hash and the
/// summary on success, the tab's name when a dirty tab stopped it.
#[test]
fn a_commit_s_notices_name_the_hash_and_the_tab() {
    for language in Language::ALL {
        let ui = Ui(*language);
        let done = ui.notice_committed("a1b2c3d", "Fix the thing");
        assert!(
            done.contains("a1b2c3d") && done.contains("Fix the thing"),
            "{done}"
        );
        let refused = ui.commit_refused_by("lib.rs");
        assert!(refused.contains("lib.rs"), "{refused}");
    }
}

/// The diff's toolbar counts hunks and files in the language.
#[test]
fn the_diff_toolbar_counts_in_the_language() {
    assert_eq!(Ui(Language::English).differences(1), "1 difference");
    assert_eq!(Ui(Language::English).differences(3), "3 differences");
    assert_eq!(Ui(Language::Russian).differences(1), "1 отличие");
    assert_eq!(Ui(Language::Russian).differences(3), "3 отличия");
    assert_eq!(Ui(Language::Russian).differences(7), "7 отличий");
    assert_eq!(Ui(Language::English).files_counter(2, 16), "2 / 16 files");
    assert_eq!(Ui(Language::Russian).files_counter(2, 16), "2 / 16 файлов");
}

/// The line above the message box counts what is ticked, in the language.
#[test]
fn the_modified_line_counts_in_the_language() {
    assert_eq!(Ui(Language::English).modified_count(1), "1 modified");
    assert_eq!(Ui(Language::English).modified_count(14), "14 modified");
    assert_eq!(Ui(Language::Russian).modified_count(1), "1 изменён");
    assert_eq!(Ui(Language::Russian).modified_count(14), "14 изменено");
    assert_eq!(Ui(Language::Russian).modified_count(3), "3 изменено");
}
/// A row in the session list must say the same truth as the counter
/// above it, using the transcript's cost or token usage.
#[test]
fn a_session_row_shows_what_it_spent() {
    for language in Language::ALL {
        let ui = Ui(*language);

        assert_eq!(
            ui.session_spent(Usage {
                cost_usd: Some(0.0125),
                ..Usage::default()
            }),
            "$0.0125",
            "finer than the counter: a single session is often fractions of a cent"
        );

        let tokens = ui.session_spent(Usage {
            output_tokens: 4_231,
            ..Usage::default()
        });
        assert!(tokens.contains("4.2k"), "{tokens}");
        assert!(!tokens.contains('$'), "no invented money: {tokens}");

        assert_eq!(
            ui.session_spent(Usage::default()),
            "",
            "nothing spent says nothing, not zero"
        );
    }
}

/// The working tree's tab counts its files in each language's own way:
/// English has one plural, Russian three forms and the teens' exception.
#[test]
fn the_diff_title_counts_files_in_the_language() {
    let english = Ui(Language::English);
    assert_eq!(english.diff_files(1), "1 file");
    assert_eq!(english.diff_files(3), "3 files");
    assert_eq!(english.diff_title(3), "Diff · 3 files");
    assert_eq!(english.diff_title(0), "Diff", "nothing to count");

    let russian = Ui(Language::Russian);
    assert_eq!(russian.diff_files(1), "1 файл");
    assert_eq!(russian.diff_files(3), "3 файла");
    assert_eq!(russian.diff_files(5), "5 файлов");
    assert_eq!(russian.diff_files(11), "11 файлов");
    assert_eq!(russian.diff_files(21), "21 файл");
    assert_eq!(russian.diff_files(112), "112 файлов");
}

/// Every tool has a name of its own, in every language.
///
/// A tool added to `Tool::ALL` without one used to fall through to
/// *Nothing here* — the line a dock with no tool at all draws — and the
/// panel would then be titled with the sentence that says it is empty.
/// M8.4 added the fourth tool, which is when this became worth a test.
#[test]
fn every_tool_is_named_in_every_language() {
    for tool in guild_session::Tool::ALL {
        let text = guild_desktop::view::panels::title_of(*tool);
        assert_ne!(
            text,
            Text::DockEmpty,
            "{tool:?} has no name of its own and would be titled *Nothing here*"
        );
        for language in Language::ALL {
            assert!(
                !Ui(*language).text(text).trim().is_empty(),
                "{tool:?} is unnamed in {language:?}"
            );
        }
    }
}

/// **A row's age is rounded down and never exact** — M12 task 16. The
/// question a worktree's row answers is *when was I last here*, and to
/// that *2h* and *2h 14m* are the same answer while the second costs the
/// width the branch is drawn in.
#[test]
fn how_long_ago_is_said_in_one_unit() {
    let en = Ui(Language::English);
    assert_eq!(en.ago(0), "just now");
    assert_eq!(en.ago(59), "just now");
    assert_eq!(en.ago(60), "1m ago");
    assert_eq!(en.ago(59 * 60 + 59), "59m ago");
    assert_eq!(en.ago(60 * 60), "1h ago");
    assert_eq!(en.ago(25 * 60 * 60), "1d ago");
    assert_eq!(en.ago(8 * 24 * 60 * 60), "1w ago");

    // Both languages answer in their own words, and neither says more
    // than one unit.
    let ru = Ui(Language::Russian);
    assert_eq!(ru.ago(30), "только что");
    assert_eq!(ru.ago(90), "1 мин назад");
    assert_eq!(ru.ago(3 * 60 * 60), "3 ч назад");
}

/// A notice is a sentence standing on its own, and starts with a capital.
#[test]
fn a_notice_starts_with_a_capital() {
    for language in Language::ALL {
        let ui = Ui(*language);
        for text in Text::ALL
            .iter()
            .filter(|text| format!("{text:?}").starts_with("Notice"))
        {
            assert!(
                ui.text(*text)
                    .chars()
                    .next()
                    .is_some_and(char::is_uppercase),
                "{text:?} in {} starts in lower case",
                language.endonym()
            );
        }
    }
}

/// *The last area stays* is said twice: at the right of the `⋮`'s faint
/// *Close Area*, in lower case as a row's reason is, and as the notice the
/// View menu's *Close Area* posts on the last area, which is a sentence.
/// The notice reused the row's words and read as clipped (M12.8's window
/// check, F4).
#[test]
fn the_last_area_is_a_reason_in_a_row_and_a_sentence_in_a_notice() {
    for language in Language::ALL {
        let ui = Ui(*language);
        let row = ui.text(Text::LastAreaStays);
        let notice = ui.text(Text::NoticeLastAreaStays);
        assert!(row.chars().next().is_some_and(char::is_lowercase));
        assert!(notice.chars().next().is_some_and(char::is_uppercase));
        assert_eq!(row.to_lowercase(), notice.to_lowercase());
    }
}

/// OSC 7501's `blocked`: a reason is said only when the program said it —
/// sign-in has its own words, and a wait with no `kind` invents none.
#[test]
fn a_waiting_reason_is_said_only_when_the_program_said_it() {
    use guild_session::status::{Status, Why};
    for language in [Language::English, Language::Russian] {
        let ui = Ui(language);
        let said: Vec<&str> = [Why::Permission, Why::Question, Why::Auth, Why::Unstated]
            .into_iter()
            .map(|why| ui.activity_state(Status::Waiting(why)))
            .collect();
        for (n, text) in said.iter().enumerate() {
            assert!(!text.is_empty());
            assert!(
                said.iter().skip(n + 1).all(|other| other != text),
                "{language:?}: {said:?}"
            );
        }
        let bare = ui
            .activity_state(Status::Waiting(Why::Unstated))
            .to_lowercase();
        for reason in ["permission", "answer", "sign", "разреш", "ответ", "вход"] {
            assert!(
                !bare.contains(reason),
                "{language:?} invents a reason: {bare}"
            );
        }
    }
    assert_eq!(
        Ui(Language::English).activity_state(Status::Waiting(Why::Auth)),
        "Waiting for sign-in"
    );
    assert_eq!(
        Ui(Language::Russian).activity_state(Status::Waiting(Why::Auth)),
        "Ждёт входа"
    );
}
