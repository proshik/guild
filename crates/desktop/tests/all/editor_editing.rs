//! The line-editing actions in a file editor: the text transformations as
//! pure functions, and the strokes through the production keymap.
use guild_desktop::editor::comments::{self, Comment};
use guild_desktop::editor::editing::{self, Edit};
use std::ops::Range;

/// `|` marks the caret, `[` and `]` a selection — the same notation for
/// what goes in and what is expected to come out.
fn parse(marked: &str) -> (String, Range<usize>, usize) {
    let mut text = String::new();
    let (mut start, mut end, mut caret) = (None, None, None);
    for ch in marked.chars() {
        match ch {
            '|' => caret = Some(text.len()),
            '[' => start = Some(text.len()),
            ']' => end = Some(text.len()),
            _ => text.push(ch),
        }
    }
    match (start, end, caret) {
        (Some(s), Some(e), None) => (text, s..e, e),
        (None, None, Some(c)) => (text, c..c, c),
        _ => panic!("mark a caret or a selection: {marked}"),
    }
}

fn mark(text: &str, selection: &Range<usize>) -> String {
    if selection.is_empty() {
        format!("{}|{}", &text[..selection.start], &text[selection.start..])
    } else {
        format!(
            "{}[{}]{}",
            &text[..selection.start],
            &text[selection.clone()],
            &text[selection.end..]
        )
    }
}

fn run(op: impl Fn(&str, Range<usize>, usize) -> Option<Edit>, before: &str) -> Option<String> {
    let (text, selection, caret) = parse(before);
    let edit = op(&text, selection, caret)?;
    let after = edit.apply(&text);
    assert!(after.is_char_boundary(edit.selection.start));
    assert!(after.is_char_boundary(edit.selection.end));
    Some(mark(&after, &edit.selection))
}

fn check(op: impl Fn(&str, Range<usize>, usize) -> Option<Edit>, cases: &[(&str, Option<&str>)]) {
    for (before, after) in cases {
        assert_eq!(run(&op, before).as_deref(), *after, "from {before:?}");
    }
}

#[test]
fn delete_line_removes_every_touched_line_and_keeps_the_column() {
    check(
        editing::delete_lines,
        &[
            ("one\nдв|а\nthree\n", Some("one\nth|ree\n")),
            ("one\nдлинная|\nx\n", Some("one\nx|\n")),
            ("o[ne\ntw]o\nthree", Some("th|ree")),
            // A selection ending at a line's start does not touch that line.
            ("[one\n]two\n", Some("|two\n")),
            // The last line takes the preceding newline with it.
            ("one\nt|wo", Some("o|ne")),
            ("\tодин\n\tдв|а", Some("\tод|ин")),
            ("on|ly", Some("|")),
            ("|", None),
            ("one\r\ntw|o\r\nthree\r\n", Some("one\r\nth|ree\r\n")),
            ("one\r\ntw|o", Some("on|e")),
        ],
    );
}

#[test]
fn duplicate_copies_the_line_below_or_the_selection_after_itself() {
    check(
        editing::duplicate,
        &[
            ("a\nмир| x\nb\n", Some("a\nмир x\nмир| x\nb\n")),
            ("a\nla|st", Some("a\nlast\nla|st")),
            ("a\r\nla|st", Some("a\r\nlast\r\nla|st")),
            ("x\r\nm|id\r\ny", Some("x\r\nmid\r\nm|id\r\ny")),
            ("f([ая])", Some("f(ая[ая])")),
            ("[one\ntwo]\n", Some("one\ntwo[one\ntwo]\n")),
        ],
    );
}

#[test]
fn move_lines_swaps_the_touched_block_with_its_neighbour() {
    let up = |t: &str, s, c| editing::move_lines(t, s, c, true);
    let down = |t: &str, s, c| editing::move_lines(t, s, c, false);
    check(
        up,
        &[
            ("a\nб|б\nc\n", Some("б|б\na\nc\n")),
            ("a\nb\nl|ast", Some("a\nl|ast\nb")),
            ("a\n[b\nc]\nd", Some("[b\nc]\na\nd")),
            ("fi|rst\nb", None),
            ("a\r\nb|b\r\nc", Some("b|b\r\na\r\nc")),
        ],
    );
    check(
        down,
        &[
            ("a\nб|б\nc\n", Some("a\nc\nб|б\n")),
            ("a\nb|b\nlast", Some("a\nlast\nb|b")),
            ("[a\nb]\nc", Some("c\n[a\nb]")),
            ("a\nla|st", None),
            ("\tx|\n\ty", Some("\ty\n\tx|")),
        ],
    );
}

#[test]
fn join_lines_trims_and_separates_with_one_space_where_it_reads() {
    check(
        editing::join_lines,
        &[
            ("let| x =   \n    value;\n", Some("let x =| value;\n")),
            ("call(a,|\n   b\n)\n", Some("call(a,| b\n)\n")),
            ("f(x|\n  )", Some("f(x|)")),
            ("a|\n  .b()", Some("a|.b()")),
            ("a|\n  , b", Some("a|, b")),
            ("a|\n  ;", Some("a|;")),
            ("a|\n   \nb", Some("a|\nb")),
            // A blank current line is joined onto with no separator.
            ("   |\n  foo\nb", Some("|foo\nb")),
            ("[\t\n  \n  foo]", Some("|foo")),
            ("[один\n  два\n\tтри]\nfour", Some("один два| три\nfour")),
            (
                "one\r\n  t|wo\r\n  three\r\n",
                Some("one\r\n  two| three\r\n"),
            ),
            ("la|st", None),
        ],
    );
}

const RUST: Comment = Comment {
    line: Some("//"),
    block: Some(("/*", "*/")),
    nodes: &["block_comment"],
};
const HTML: Comment = Comment {
    line: None,
    block: Some(("<!--", "-->")),
    nodes: &["comment"],
};
const PYTHON: Comment = Comment {
    line: Some("#"),
    block: None,
    nodes: &[],
};
const NONE: Comment = Comment {
    line: None,
    block: None,
    nodes: &[],
};

#[test]
fn line_comment_toggles_at_the_blocks_indentation_and_steps_down() {
    let rust = |t: &str, s, c| editing::toggle_line_comment(t, s, c, RUST);
    check(
        rust,
        &[
            (
                "fn f() {\n    le|t x;\n}\n",
                Some("fn f() {\n    // let x;\n}|\n"),
            ),
            ("    // le|t x;\n}\n", Some("    let x;\n}|\n")),
            ("    //le|t x;\n}", Some("    let x;\n}|")),
            ("    la|st", Some("    // la|st")),
            // A mixed block is commented, every line at the smallest indent.
            (
                "[    a\n\n        // b\n      c]\n",
                Some("[    // a\n\n    //     // b\n    //   c]\n"),
            ),
            (
                "[    // a\n\n    //     // b\n    //   c]\n",
                Some("[    a\n\n        // b\n      c]\n"),
            ),
            ("\tмир|\n", Some("\t// мир\n|")),
            ("   |   \nx", None),
            ("a|\r\nb\r\n", Some("// a\r\nb|\r\n")),
        ],
    );
    let html = |t: &str, s, c| editing::toggle_line_comment(t, s, c, HTML);
    check(
        html,
        &[
            ("  <p>|</p>\nx", Some("  <!-- <p></p> -->\nx|")),
            ("  <!-- <p>|</p> -->\nx", Some("  <p></p>\nx|")),
        ],
    );
    let py = |t: &str, s, c| editing::toggle_line_comment(t, s, c, PYTHON);
    check(py, &[("[a\nb]", Some("[# a\n# b]"))]);
    let none = |t: &str, s, c| editing::toggle_line_comment(t, s, c, NONE);
    check(none, &[("a|", None)]);
}

/// The block-comment toggle as the editor runs it: the file's syntax asked
/// first, then the transformation.
fn block(grammar: &'static str) -> impl Fn(&str, Range<usize>, usize) -> Option<Edit> {
    move |t: &str, s: Range<usize>, c| {
        let around = editing::comment_around(t, grammar, s.clone());
        editing::toggle_block_comment(t, s, c, comments::for_grammar(grammar), around)
    }
}

#[test]
fn block_comment_wraps_unwraps_or_inserts_an_empty_pair() {
    check(
        block("rust"),
        &[
            ("f(|)", Some("f(/*|*/)")),
            ("f([мир])", Some("f([/*мир*/])")),
            ("f([/*мир*/])", Some("f([мир])")),
            ("f(/* а [б] в */)", Some("f( а [б] в )")),
            ("/* a */ [b] /* c */", Some("/* a */ [/*b*/] /* c */")),
        ],
    );
    check(
        block("html"),
        &[
            ("[<p/>]", Some("[<!--<p/>-->]")),
            ("<!-- a [b] c -->", Some(" a [b] c ")),
        ],
    );
    check(block("python"), &[("[x]", None), ("|", None)]);
}

/// Two comments with code between are not one comment: the selection is
/// wrapped, never stripped of the first open and the last close — and the
/// wrapped result, one nested comment to Rust, unwraps back.
#[test]
fn block_comment_never_unwraps_two_comments_as_one() {
    check(
        block("rust"),
        &[
            (
                "[/* a */ let x = 1; /* b */]",
                Some("[/*/* a */ let x = 1; /* b */*/]"),
            ),
            (
                "[/*/* a */ let x = 1; /* b */*/]",
                Some("[/* a */ let x = 1; /* b */]"),
            ),
            ("[ /* a */ ]", Some("[  a  ]")),
        ],
    );
    // Without a tree only the text can be asked, and it refuses to take two
    // comments for one.
    let textual = |t: &str, s: Range<usize>, c| {
        editing::toggle_block_comment(t, s, c, RUST, editing::Around::Unknown)
    };
    check(
        textual,
        &[
            (
                "[/* a */ let x = 1; /* b */]",
                Some("[/*/* a */ let x = 1; /* b */*/]"),
            ),
            ("[ /* a */ ]", Some("[  a  ]")),
            ("x [/* a */] y", Some("x [ a ] y")),
        ],
    );
}

/// A whole nested comment is one comment node, and unwraps as one.
#[test]
fn a_whole_nested_comment_unwraps() {
    check(
        block("rust"),
        &[
            (
                "x(); [/* outer /* inner */ tail */]",
                Some("x(); [ outer /* inner */ tail ]"),
            ),
            (
                "x(); /* outer /* inner */ [tail] */",
                Some("x();  outer /* inner */ [tail] "),
            ),
        ],
    );
}

/// The node that carries the delimiters is the comment, not the doc or
/// content node inside it.
#[test]
fn the_enclosing_comment_is_the_node_with_the_delimiters() {
    check(
        block("rust"),
        &[
            ("x(); /** hello [world] */", Some("x(); * hello [world] ")),
            ("x(); /*! hello [world] */", Some("x(); ! hello [world] ")),
        ],
    );
    // Lua's own brackets would read as the marks: spelled out by hand.
    let lua = "x = 1 --[[ hello world ]]";
    let world = lua.find("world").unwrap();
    let edit = block("lua")(lua, world..world + 5, world + 5).unwrap();
    assert_eq!(edit.apply(lua), "x = 1  hello world ");
    assert_eq!(edit.selection, world - 4..world + 1);
    check(
        block("xml"),
        &[(
            "<root><!-- a [value] b --></root>",
            Some("<root> a [value] b </root>"),
        )],
    );
}

/// The tokens inside string literals are not a comment, and only the
/// file's syntax can tell: the selection between them is wrapped.
#[test]
fn block_comment_tokens_in_strings_do_not_enclose_a_selection() {
    let text = "let a = \"/*\";\nlet value = 1;\nlet b = \"*/\";\n";
    let value = text.find("value").unwrap();
    assert_eq!(
        editing::comment_around(text, "rust", value..value + 5),
        editing::Around::Nothing
    );
    check(
        block("rust"),
        &[
            (
                "let a = \"/*\";\nlet [value] = 1;\nlet b = \"*/\";\n",
                Some("let a = \"/*\";\nlet [/*value*/] = 1;\nlet b = \"*/\";\n"),
            ),
            // Exactly the tokens' span, but strings: the syntax says no.
            ("f(\"[/*\", \"*/]\")", Some("f(\"[/*/*\", \"*/*/]\")")),
        ],
    );
    // A real comment around it is found by the grammar…
    let text = "let a = 1; /* let value = 1; */\n";
    let value = text.find("value").unwrap();
    assert_eq!(
        editing::comment_around(text, "rust", value..value + 5),
        editing::Around::Comment(11..31)
    );
    // …and not by plain text, nor past the highlighting limit, where the
    // editor is plain text too: there nothing is unwrapped from inside.
    assert_eq!(
        editing::comment_around(text, "text", value..value + 5),
        editing::Around::Unknown
    );
    let limit = guild_desktop::file_tab::HIGHLIGHT_LIMIT as usize;
    let big = format!("/* {} */\n", "x ".repeat(limit / 2 + 1));
    assert_eq!(
        editing::comment_around(&big, "rust", 3..4),
        editing::Around::Unknown
    );
    let edit = block("rust")(&big, 3..4, 4).unwrap();
    assert_eq!(&edit.apply(&big)[..8], "/* /*x*/");
}

/// **Every grammar with a block comment names its comment node**, checked
/// against the grammar itself: a block comment in each one is found from a
/// word inside it. A grammar whose tree has no comment node — Markdown's
/// `<!-- -->` is an `html_block` — is listed, and asks the text instead.
#[test]
fn every_block_comment_grammar_names_its_real_comment_node() {
    const TEXT_ONLY: &[&str] = &["markdown"];
    for (grammar, comment) in comments::COMMENTS {
        let Some((open, close)) = comment.block else {
            continue;
        };
        let c = format!("{open} a word b {close}");
        let text = match *grammar {
            "html" | "xml" | "svelte" => format!("<root>{c}</root>\n"),
            "json" => format!("{{ {c} \"a\": 1 }}\n"),
            "css" | "scss" => format!("a {{ {c} color: red; }}\n"),
            "sql" => format!("SELECT 1; {c}\n"),
            "markdown" => format!("text\n\n{c}\n"),
            "php" => format!("<?php {c} $a = 1;\n"),
            "hcl" => format!("x = 1\n{c}\ny = 2\n"),
            _ => format!("x = 1; {c}\n"),
        };
        let at = text.find("word").unwrap();
        let start = text.find(&c).unwrap();
        let found = editing::comment_around(&text, grammar, at..at + 4);
        if TEXT_ONLY.contains(grammar) {
            assert!(comment.nodes.is_empty(), "{grammar}");
            assert_eq!(found, editing::Around::Unknown, "{grammar}");
        } else {
            assert!(!comment.nodes.is_empty(), "{grammar} names no comment node");
            assert_eq!(
                found,
                editing::Around::Comment(start..start + c.len()),
                "{grammar}: {text:?}"
            );
        }
    }
}

#[test]
fn start_new_line_keeps_the_indentation_and_never_splits() {
    let below = |t: &str, s: Range<usize>, c| editing::start_new_line(t, s, c, false);
    let above = |t: &str, s: Range<usize>, c| editing::start_new_line(t, s, c, true);
    check(
        below,
        &[
            ("    let| x = 1;\nnext", Some("    let x = 1;\n    |\nnext")),
            ("\tмир|", Some("\tмир\n\t|")),
            ("  a[b]c\r\n", Some("  abc\r\n  |\r\n")),
        ],
    );
    check(
        above,
        &[
            ("first\n    let| x;\n", Some("first\n    |\n    let x;\n")),
            ("\tмир|", Some("\t|\n\tмир")),
        ],
    );
}

#[test]
fn toggle_case_uppercases_anything_with_a_lowercase_letter() {
    check(
        editing::toggle_case,
        &[
            ("let мир|ок = 1", Some("let МИР|ОК = 1")),
            ("let МИ|Р = 1", Some("let ми|р = 1")),
            ("[Hello World]", Some("[HELLO WORLD]")),
            ("[HELLO 42]", Some("[hello 42]")),
            ("a | b", None),
            ("[42]", None),
        ],
    );
}

/// **One table, the grammars' names.** Every registered grammar either
/// has its comment tokens here or is listed as having none — a grammar
/// added tomorrow fails this until somebody decides which.
#[test]
fn every_registered_grammar_has_comment_tokens_or_is_listed_without() {
    let names: Vec<&str> = guild_desktop::grammars::all()
        .iter()
        .map(|g| g.name)
        .collect();
    for name in &names {
        let tokens = comments::COMMENTS.iter().any(|(n, _)| n == name);
        let without = comments::COMMENTLESS.contains(name);
        assert!(
            tokens != without,
            "{name}: in the table {tokens}, listed without {without}"
        );
    }
    for (name, comment) in comments::COMMENTS {
        assert!(names.contains(name), "{name} is not a registered grammar");
        assert!(
            comment.line.is_some() || comment.block.is_some(),
            "{name} has an empty row: list it as comment-less instead"
        );
    }
    for name in comments::COMMENTLESS {
        assert!(names.contains(name), "{name} is not a registered grammar");
    }
    assert_eq!(comments::for_grammar("rust"), RUST);
    assert_eq!(comments::for_grammar("text"), NONE);
    assert_eq!(comments::for_grammar("a-grammar-nobody-has"), NONE);
}

use gpui::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use gpui_component::input::EditorState;
use guild_desktop::{
    file_tab::{FileTab, Look},
    i18n::Ui,
    theme::Theme,
};
use std::{path::Path, time::Duration};

fn open<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    source: &str,
    readonly: bool,
) -> (
    tempfile::TempDir,
    Entity<FileTab>,
    Entity<EditorState>,
    &'a mut VisualTestContext,
) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(name), source).unwrap();
    let mut made = None;
    let path = dir.path().to_owned();
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        guild_desktop::actions::install(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                &path,
                Path::new(name),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
            .read_only(readonly)
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    cx.simulate_resize(size(px(650.), px(320.)));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    let file = made.unwrap();
    let editor = cx.update(|window, cx| {
        let editor = file.read(cx).editor().unwrap().clone();
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        editor
    });
    cx.run_until_parked();
    (dir, file, editor, cx)
}

/// The action the focused element's context stack resolves `stroke` to.
fn resolved(cx: &mut VisualTestContext, stroke: &str) -> String {
    cx.update(|window, cx| {
        let contexts = window.context_stack();
        cx.key_bindings()
            .borrow()
            .bindings_for_input(&[gpui::Keystroke::parse(stroke).unwrap()], &contexts)
            .0
            .first()
            .map_or("nothing".into(), |b| b.action().name().to_owned())
    })
}

fn resolved_in(cx: &mut TestAppContext, contexts: &[&str], stroke: &str) -> String {
    cx.update(|cx| {
        let contexts = contexts
            .iter()
            .map(|c| gpui::KeyContext::parse(c).unwrap())
            .collect::<Vec<_>>();
        cx.key_bindings()
            .borrow()
            .bindings_for_input(&[gpui::Keystroke::parse(stroke).unwrap()], &contexts)
            .0
            .first()
            .map_or("nothing".into(), |b| b.action().name().to_owned())
    })
}

/// Put the editor in `marked`'s state, press `stroke`, compare with
/// `expected`, then undo once and get `marked`'s text back.
fn press(
    cx: &mut VisualTestContext,
    editor: &Entity<EditorState>,
    marked: &str,
    stroke: &str,
    expected: &str,
) {
    let (text, selection, _) = parse(marked);
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| {
            s.set_value(text.clone(), window, cx);
            s.set_selected_range(selection, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes(stroke);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        assert_eq!(
            mark(&state.value(), &state.selected_range()),
            expected,
            "{stroke} on {marked:?}"
        );
    });
    // Undo gives back the text and the selection the person had, redo the
    // text and the selection the action left.
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        assert_eq!(
            mark(&state.value(), &state.selected_range()),
            mark(&text, &parse(marked).1),
            "one cmd-z undoes {stroke}"
        )
    });
    cx.simulate_keystrokes("cmd-shift-z");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        assert_eq!(
            mark(&state.value(), &state.selected_range()),
            expected,
            "cmd-shift-z redoes {stroke}"
        )
    });
}

#[gpui::test]
fn every_stroke_reaches_an_editable_file_and_is_one_undo_step(cx: &mut TestAppContext) {
    let (_dir, file, editor, cx) = open(cx, "main.rs", "fn main() {}\n", false);
    for (stroke, action) in [
        ("cmd-backspace", "guild_editor::DeleteLine"),
        ("cmd-d", "guild_editor::Duplicate"),
        ("alt-shift-up", "guild_editor::MoveLineUp"),
        ("alt-shift-down", "guild_editor::MoveLineDown"),
        ("ctrl-shift-j", "guild_editor::JoinLines"),
        ("cmd-/", "guild_editor::CommentWithLineComment"),
        ("cmd-alt-/", "guild_editor::CommentWithBlockComment"),
        ("shift-enter", "guild_editor::StartNewLine"),
        ("cmd-alt-enter", "guild_editor::StartNewLineBeforeCurrent"),
        ("ctrl-shift-u", "guild_editor::ToggleCase"),
        // Neither is taken: multi-cursor and Go to Waiting Session keep theirs.
        ("cmd-alt-down", "input::AddCursorBelow"),
        ("cmd-shift-u", "guild::GoToWaitingSession"),
    ] {
        assert_eq!(resolved(cx, stroke), action, "{stroke}");
    }
    let rs = [
        (
            "fn a() {\n    le|t x;\n}\n",
            "cmd-backspace",
            "fn a() {\n}|\n",
        ),
        ("a\nмир| x\n", "cmd-d", "a\nмир x\nмир| x\n"),
        ("f([ая])", "cmd-d", "f(ая[ая])"),
        ("a\nб|б\nc\n", "alt-shift-up", "б|б\na\nc\n"),
        ("a\nб|б\nc\n", "alt-shift-down", "a\nc\nб|б\n"),
        (
            "let| x =   \n    value;\n",
            "ctrl-shift-j",
            "let x =| value;\n",
        ),
        (
            "    le|t x;\nfn b() {}\n",
            "cmd-/",
            "    // let x;\nfn b()| {}\n",
        ),
        ("    // le|t x;\n}\n", "cmd-/", "    let x;\n}|\n"),
        ("f([мир])", "cmd-alt-/", "f([/*мир*/])"),
        ("f(|)", "cmd-alt-/", "f(/*|*/)"),
        ("f(/* а [б] в */)", "cmd-alt-/", "f( а [б] в )"),
        (
            "let a = \"/*\";\nlet [value] = 1;\nlet b = \"*/\";",
            "cmd-alt-/",
            "let a = \"/*\";\nlet [/*value*/] = 1;\nlet b = \"*/\";",
        ),
        (
            "    let| x = 1;\n",
            "shift-enter",
            "    let x = 1;\n    |\n",
        ),
        (
            "first\n\tlet| x;\n",
            "cmd-alt-enter",
            "first\n\t|\n\tlet x;\n",
        ),
        ("let мир|ок = 1", "ctrl-shift-u", "let МИР|ОК = 1"),
    ];
    for (marked, stroke, expected) in rs {
        press(cx, &editor, marked, stroke, expected);
    }
    cx.update(|_, cx| assert!(file.read(cx).is_dirty(), "an edit dirties the file"));
}

#[gpui::test]
fn html_comments_with_its_block_tokens_line_by_line(cx: &mut TestAppContext) {
    let (_dir, _file, editor, cx) = open(cx, "page.html", "<p></p>\n", false);
    press(
        cx,
        &editor,
        "  <p>|</p>\nx",
        "cmd-/",
        "  <!-- <p></p> -->\nx|",
    );
}

#[gpui::test]
fn plain_text_has_nothing_to_comment_with(cx: &mut TestAppContext) {
    let (_dir, _file, editor, cx) = open(cx, "notes.txt", "plain\n", false);
    for stroke in ["cmd-/", "cmd-alt-/"] {
        cx.update(|_, cx| editor.update(cx, |s, cx| s.set_selected_range(3..3, cx)));
        cx.simulate_keystrokes(stroke);
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "plain\n", "{stroke}"));
    }
}

#[gpui::test]
fn a_readonly_source_keeps_its_text_and_the_component_keeps_its_strokes(cx: &mut TestAppContext) {
    let (_dir, _file, editor, cx) = open(cx, "main.rs", "fn main() {}\n", true);
    for stroke in [
        "cmd-backspace",
        "cmd-d",
        "alt-shift-up",
        "ctrl-shift-j",
        "cmd-/",
    ] {
        assert!(
            !resolved(cx, stroke).starts_with("guild_editor::"),
            "{stroke} in a read-only source"
        );
        cx.simulate_keystrokes(stroke);
    }
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "fn main() {}\n"));
}

/// **`cmd-d` is Show Diff everywhere but a buffer**, and the other strokes
/// stay what they were outside an editable file.
#[gpui::test]
fn elsewhere_the_strokes_keep_their_meaning(cx: &mut TestAppContext) {
    cx.update(|cx| {
        guild_desktop::editor::init(cx);
        guild_desktop::actions::install(cx);
    });
    for host in [
        vec!["Root", "guild", "changes"],
        vec!["Root", "guild", "tree"],
        vec!["Root", "guild", "guild-file"],
        vec!["Root", "guild", "guild-readonly-file", "Input"],
    ] {
        assert_eq!(
            resolved_in(cx, &host, "cmd-d"),
            "guild::ShowDiff",
            "{host:?}"
        );
    }
    for host in [
        vec!["Root", "guild", "guild-file", "guild-field", "Input"],
        vec!["Root", "guild", "guild-file", "guild-local-search", "Input"],
        vec!["Root", "guild", "changes", "Input"],
        vec!["Root", "guild", "terminal"],
    ] {
        for stroke in [
            "cmd-backspace",
            "cmd-d",
            "alt-shift-up",
            "ctrl-shift-j",
            "cmd-/",
            "cmd-alt-/",
            "shift-enter",
            "cmd-alt-enter",
            "ctrl-shift-u",
        ] {
            assert!(
                !resolved_in(cx, &host, stroke).starts_with("guild_editor::"),
                "{stroke} in {host:?}"
            );
        }
    }
    assert_eq!(
        resolved_in(cx, &["Root", "guild", "changes", "Input"], "cmd-backspace"),
        "input::DeleteToBeginningOfLine"
    );
    assert_eq!(
        resolved_in(cx, &["Root", "guild", "tree"], "cmd-backspace"),
        "guild::DeleteFile"
    );
}

fn state(cx: &mut VisualTestContext, editor: &Entity<EditorState>) -> String {
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        mark(&state.value(), &state.selected_range())
    })
}

fn start(cx: &mut VisualTestContext, editor: &Entity<EditorState>, marked: &str) {
    let (text, selection, _) = parse(marked);
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.set_selected_range(selection, cx);
        })
    });
    cx.run_until_parked();
}

/// **The journal follows the history's position, not its texts.** The first
/// and third Toggle Case have the same texts and different carets; undoing
/// all three gives each one's own caret back, and redo the action's.
#[gpui::test]
fn repeated_transitions_restore_each_actions_own_selection(cx: &mut TestAppContext) {
    let (_dir, _file, editor, cx) = open(cx, "main.rs", "x\n", false);
    start(cx, &editor, "|ab");
    cx.simulate_keystrokes("ctrl-shift-u");
    assert_eq!(state(cx, &editor), "|AB");
    cx.simulate_keystrokes("right ctrl-shift-u");
    assert_eq!(state(cx, &editor), "a|b");
    cx.simulate_keystrokes("ctrl-shift-u");
    assert_eq!(state(cx, &editor), "A|B");
    for expected in ["a|b", "A|B", "|ab"] {
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(state(cx, &editor), expected);
    }
    for expected in ["|AB", "a|b", "A|B"] {
        cx.simulate_keystrokes("cmd-shift-z");
        assert_eq!(state(cx, &editor), expected);
    }
}

/// **A change of the person's own ends the journal.** Typing after an undo
/// drops the redo branch in the component; the journal must not then take
/// the typing's undo for the duplicate's and select the letter.
#[gpui::test]
fn typing_after_an_undo_leaves_the_components_selection(cx: &mut TestAppContext) {
    let (_dir, _file, editor, cx) = open(cx, "main.rs", "x\n", false);
    start(cx, &editor, "[a]");
    cx.simulate_keystrokes("cmd-d");
    assert_eq!(state(cx, &editor), "a[a]");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(cx, &editor), "[a]");
    cx.simulate_keystrokes("right");
    // The same letter: `aa` back to `a` is the duplicate's texts exactly.
    cx.simulate_input("a");
    assert_eq!(state(cx, &editor), "aa|");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(cx, &editor), "a|");
}

/// **As deep as the component's history.** A hundred and one actions undo
/// one by one, the last back to the caret the person had — not the helper
/// range of the deleted line.
#[gpui::test]
fn a_hundred_and_one_actions_undo_to_the_first_caret(cx: &mut TestAppContext) {
    let (_dir, _file, editor, cx) = open(cx, "main.rs", "x\n", false);
    start(cx, &editor, "a|bc\ndef");
    cx.simulate_keystrokes("cmd-backspace");
    assert_eq!(state(cx, &editor), "d|ef");
    for _ in 0..100 {
        cx.simulate_keystrokes("cmd-d");
    }
    for _ in 0..100 {
        cx.simulate_keystrokes("cmd-z");
    }
    assert_eq!(state(cx, &editor), "d|ef");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(cx, &editor), "a|bc\ndef");
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(state(cx, &editor), "d|ef");
}

/// A value set from outside — a reload — is not the journal's either.
#[gpui::test]
fn a_replaced_buffer_ends_the_journal(cx: &mut TestAppContext) {
    let (_dir, _file, editor, cx) = open(cx, "main.rs", "x\n", false);
    start(cx, &editor, "|ab");
    cx.simulate_keystrokes("ctrl-shift-u");
    start(cx, &editor, "A|B");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(cx, &editor), "A|B");
}
