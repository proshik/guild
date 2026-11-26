//! The preview's excerpt and its marks — M8.9 group C's task 14.
//!
//! A preview is the code around a hit, painted by `desktop::syntax` rather
//! than by the editor component (decision 13). What can be said without a
//! window is here: which lines, in which colours, and where a match is
//! marked.

use std::ops::Range;

use guild_desktop::preview;

/// A text of `count` numbered lines.
fn numbered(count: usize) -> String {
    (0..count).map(|at| format!("line {at}\n")).collect()
}

/// **The excerpt is centred on the line, and slides at the ends** rather
/// than coming up short: the hit near the top of a file still has a full
/// window of code under it.
#[test]
fn the_excerpt_is_centred_on_the_line_and_slides_at_the_ends() {
    let text = numbered(100);

    let middle = preview::excerpt(&text, None, 50, 3);
    assert_eq!(middle.first, 47);
    assert_eq!(middle.lines.len(), 7);
    assert_eq!(middle.lines[3], "line 50", "the line in the middle");

    let top = preview::excerpt(&text, None, 1, 3);
    assert_eq!(top.first, 0, "slid down from the top");
    assert_eq!(top.lines.len(), 7);
    assert_eq!(top.lines[1], "line 1");

    let bottom = preview::excerpt(&text, None, 99, 3);
    assert_eq!(bottom.first, 93, "slid up from the end");
    assert_eq!(bottom.lines.len(), 7);
    assert_eq!(bottom.lines[6], "line 99");

    let short = preview::excerpt("one\ntwo\n", None, 1, 3);
    assert_eq!(short.first, 0);
    assert_eq!(short.lines, vec!["one", "two"], "a short file whole");

    let past = preview::excerpt(&text, None, 500, 3);
    assert_eq!(past.first, 93, "a line past the end shows the end");

    let crlf = preview::excerpt("a\r\nb\r\n", None, 0, 1);
    assert_eq!(crlf.lines, vec!["a", "b"], "no carriage return drawn");
    assert_eq!(
        crlf.spans.len(),
        crlf.lines.len(),
        "a row of spans per line"
    );
}

/// **A match is marked only on a line that is the hit's own text** —
/// `SidePaint::of_line`'s rule, met a third time. A file changed under the
/// popup moves its lines, and marks painted on whatever line is there now
/// would put brass on words that never matched.
#[test]
fn a_match_is_marked_only_on_a_line_that_is_the_hit_s_text() {
    let ranges: Vec<Range<usize>> = std::iter::once(7..17).collect();
    let hit = ("pub fn needle_one() {}".to_owned(), ranges.clone());

    assert_eq!(
        preview::marks("pub fn needle_one() {}", &hit),
        Some(ranges.as_slice())
    );
    assert_eq!(preview::marks("fn other() {}", &hit), None);
    assert_eq!(preview::marks("pub fn needle_one() {} ", &hit), None);
}

/// **A file over the highlight limit is previewed plain**, the file tab's
/// own limit: a file coloured in the preview and plain in its tab would be
/// two answers to one question, and tree-sitter on megabytes is a wait.
#[test]
fn a_file_over_the_limit_is_previewed_plain() {
    let small = "fn main() {\n    let x = 1;\n}\n";
    let painted = preview::excerpt(small, Some("rust"), 1, 2);
    assert!(
        painted
            .spans
            .iter()
            .any(|line| line.iter().any(|span| span.paint.is_some())),
        "a small Rust file is coloured"
    );

    let mut big = String::from("fn main() {}\n");
    while (big.len() as u64) <= guild_desktop::file_tab::HIGHLIGHT_LIMIT {
        big.push_str("// needle_one and more words to make it long enough\n");
    }
    let plain = preview::excerpt(&big, Some("rust"), 0, 2);
    assert_eq!(plain.lines[0], "fn main() {}");
    assert!(
        plain
            .spans
            .iter()
            .all(|line| line.iter().all(|span| span.paint.is_none())),
        "past the limit, plain"
    );
}

/// **A long line is cut on a character boundary, spans and all** — the trap
/// *a span is a byte range*: GPUI refuses a run that starts inside a
/// character, and a Cyrillic comment in a minified file would take the
/// window down on the first preview of it.
#[test]
fn a_cyrillic_line_is_cut_on_character_boundaries() {
    let long = format!("// {}\nfn main() {{}}\n", "ёж".repeat(4000));
    let excerpt = preview::excerpt(&long, Some("rust"), 0, 1);

    let line = &excerpt.lines[0];
    assert!(line.len() < 8000, "cut");
    assert!(line.len() <= preview::LINE_LIMIT);
    assert!(line.is_char_boundary(line.len()));
    for span in &excerpt.spans[0] {
        assert!(span.at.end <= line.len(), "{span:?} inside the cut line");
        assert!(
            line.is_char_boundary(span.at.start),
            "{span:?} starts on a character"
        );
        assert!(
            line.is_char_boundary(span.at.end),
            "{span:?} ends on a character"
        );
    }
    assert_eq!(excerpt.lines[1], "fn main() {}");
}

/// **A mark is painted over the colours it crosses, and the colours go on
/// either side of it.** The runs a preview line is drawn with cover it
/// once each, in order, so a match inside a string is brass and the rest
/// of the string is still a string.
#[test]
fn a_mark_is_painted_over_the_colours_it_crosses() {
    let excerpt = preview::excerpt("let s = \"a needle b\";\n", Some("rust"), 0, 0);
    let line = &excerpt.lines[0];
    let at = line.find("needle").expect("the word");
    let marks: Vec<Range<usize>> = std::iter::once(at..at + "needle".len()).collect();

    let runs = preview::runs(&excerpt.spans[0], &marks, line.len());

    let mut end = 0;
    for run in &runs {
        assert_eq!(run.at.start, end, "in order, no gaps, no overlap");
        end = run.at.end;
    }
    assert_eq!(end, line.len(), "the whole line");
    let marked: Vec<&str> = runs
        .iter()
        .filter(|run| run.marked)
        .map(|run| &line[run.at.clone()])
        .collect();
    assert_eq!(marked, vec!["needle"]);
    let string_paint = runs
        .iter()
        .find(|run| &line[run.at.clone()] == " b\"")
        .map(|run| run.paint);
    assert!(
        string_paint.flatten().is_some(),
        "the string goes on after the mark: {runs:?}"
    );

    let plain = preview::runs(&[], &marks, line.len());
    assert_eq!(plain.iter().filter(|run| run.marked).count(), 1);
    assert!(plain.iter().all(|run| run.paint.is_none()));
}

#[test]
fn overlapping_out_of_order_preview_marks_keep_their_union_and_syntax() {
    let text = "let name=42;";
    let excerpt = preview::excerpt(text, Some("rust"), 0, 0);
    let runs = preview::runs(&excerpt.spans[0], &[6..9, 4..8, 9..11], text.len());
    assert_eq!(
        runs.iter()
            .filter(|r| r.marked)
            .map(|r| &text[r.at.clone()])
            .collect::<String>(),
        "name=42"
    );
    assert_eq!(runs.first().unwrap().at.start, 0);
    assert_eq!(runs.last().unwrap().at.end, text.len());
    assert!(
        runs.windows(2)
            .all(|pair| pair[0].at.end == pair[1].at.start)
    );
    assert!(
        runs.iter()
            .any(|run| !run.marked && run.paint.is_some() && text[run.at.clone()].contains("let"))
    );
}

/// **A popup's preview is drawn as the editor draws code** — the code
/// size from the settings at the code leading, 12.5 × 1.70 = 21.25 a line,
/// where it was `meta` 11.5 at GPUI's default 18.6. The editor and the
/// preview both read this one `Look`, so the two cannot drift apart again.
#[test]
fn a_preview_line_is_the_editor_s_line() {
    let look = guild_desktop::file_tab::Look {
        family: "Lilex".into(),
        font_size: 12.5,
        theme: guild_desktop::theme::Theme::default(),
        autosave: false,
    };
    assert_eq!(look.text_size(), gpui::px(12.5));
    assert_eq!(look.line_height(), gpui::px(21.25));
    let larger = guild_desktop::file_tab::Look {
        font_size: 15.0,
        ..look
    };
    assert_eq!(
        larger.line_height(),
        gpui::px(25.5),
        "it follows the setting"
    );
}
