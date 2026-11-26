//! Snapshot-based matching shared by local and project replacement.
use crate::search::Query;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub range: Range<usize>,
    pub replacement: String,
}

pub fn matches(
    text: &str,
    query: &Query,
    replacement: &str,
    preserve_case: bool,
) -> Result<Vec<Match>, String> {
    matches_cancellable(text, query, replacement, preserve_case, || false)
        .map(|found| found.expect("uncancelled matching"))
}

/// Cancellation is checked before collection and on every match. None is a
/// cancelled result, never a partial replacement plan.
pub(crate) fn matches_cancellable(
    text: &str,
    query: &Query,
    replacement: &str,
    preserve_case: bool,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<Vec<Match>>, String> {
    collect_matches(
        text,
        query,
        replacement,
        preserve_case,
        |_| Pick::Take,
        &mut cancelled,
    )
}

#[derive(Clone, Copy)]
enum Pick {
    Skip,
    Take,
    Last,
    Stop,
}

/// Find/navigation never interpolate a hypothetical replacement plan.
pub fn find(text: &str, query: &Query) -> Result<Vec<Match>, String> {
    matches(text, query, "", false)
}

pub(crate) fn selected_plan(
    text: &str,
    query: &Query,
    replacement: &str,
    preserve_case: bool,
    selected: &[Match],
) -> Result<Vec<Match>, String> {
    let mut index = 0;
    collect_matches(
        text,
        query,
        replacement,
        preserve_case,
        |range| {
            let Some(wanted) = selected.get(index) else {
                return Pick::Stop;
            };
            if *range != wanted.range {
                return Pick::Skip;
            }
            index += 1;
            if index == selected.len() {
                Pick::Last
            } else {
                Pick::Take
            }
        },
        || false,
    )
    .map(|found| found.expect("uncancelled selected plan"))
}

pub(crate) fn first_on_line(
    text: &str,
    query: &Query,
    replacement: &str,
    preserve_case: bool,
    line: u32,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<Vec<Match>>, String> {
    let mut start = 0;
    let mut current = 1;
    let mut end = text.len();
    // Locate this line once; cancellation remains available inside a long prefix.
    for (at, byte) in text.bytes().enumerate() {
        if at % 4096 == 0 && cancelled() {
            return Ok(None);
        }
        if byte == b'\n' {
            if current == line {
                end = at;
                break;
            }
            current += 1;
            start = at + 1;
        }
    }
    if current != line {
        return Ok(Some(Vec::new()));
    }
    collect_matches(
        text,
        query,
        replacement,
        preserve_case,
        |range| {
            if range.start < start {
                Pick::Skip
            } else if range.start <= end {
                Pick::Last
            } else {
                Pick::Stop
            }
        },
        cancelled,
    )
}

fn collect_matches(
    text: &str,
    query: &Query,
    replacement: &str,
    preserve_case: bool,
    mut pick: impl FnMut(&Range<usize>) -> Pick,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<Vec<Match>>, String> {
    use grep_matcher::{Captures, Matcher};
    if cancelled() {
        return Ok(None);
    }
    if query.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let matcher = grep_regex::RegexMatcherBuilder::new()
        .case_insensitive(!query.case_sensitive)
        .fixed_strings(!query.regular_expression)
        .word(query.whole_word)
        .multi_line(true)
        .line_terminator(None)
        .build(&query.text)
        .map_err(|error| error.to_string())?;
    let mut captures = matcher.new_captures().map_err(|error| error.to_string())?;
    let mut found = Vec::new();
    let mut stopped = false;
    let mut capped = false;
    let mut invalid_utf8 = false;
    let mut oversized = false;
    let mut inserted_bytes = 0usize;
    let mut removed_bytes = 0usize;
    matcher
        .captures_iter(text.as_bytes(), &mut captures, |captures| {
            if cancelled() {
                stopped = true;
                return false;
            }
            if found.len() >= 100_000 {
                capped = true;
                return false;
            }
            let Some(range) = captures.get(0) else {
                return true;
            };
            if !text.is_char_boundary(range.start()) || !text.is_char_boundary(range.end()) {
                return true;
            }
            let choice = pick(&(range.start()..range.end()));
            match choice {
                Pick::Skip => return true,
                Pick::Stop => return false,
                _ => {}
            }
            let mut bytes = Vec::new();
            if query.regular_expression {
                captures.interpolate(
                    |name| matcher.capture_index(name),
                    text.as_bytes(),
                    replacement.as_bytes(),
                    &mut bytes,
                );
            } else {
                bytes.extend_from_slice(replacement.as_bytes());
            }
            let Ok(mut replacement) = String::from_utf8(bytes) else {
                invalid_utf8 = true;
                return false;
            };
            if preserve_case && !query.regular_expression {
                replacement = case_like(&text[range.start()..range.end()], &replacement);
            }
            removed_bytes += range.end() - range.start();
            inserted_bytes = inserted_bytes.saturating_add(replacement.len());
            if inserted_bytes > text.len().max(crate::file_tab::OPEN_LIMIT as usize) {
                oversized = true;
                return false;
            }
            found.push(Match {
                range: range.start()..range.end(),
                replacement,
            });
            !matches!(choice, Pick::Last)
        })
        .map_err(|error| error.to_string())?;
    if stopped || cancelled() {
        return Ok(None);
    }
    if invalid_utf8 {
        return Err("Replacement capture is not valid UTF-8; no edits applied".into());
    }
    if oversized
        || text
            .len()
            .saturating_sub(removed_bytes)
            .saturating_add(inserted_bytes)
            > text.len().max(crate::file_tab::OPEN_LIMIT as usize)
    {
        return Err("Replacement exceeds the editor size limit; narrow the search".into());
    }
    if capped {
        return Err("Too many matches; narrow the search before replacing".into());
    }
    Ok(Some(found))
}

fn case_like(original: &str, replacement: &str) -> String {
    let letters: Vec<_> = original.chars().filter(|c| c.is_alphabetic()).collect();
    if !letters.is_empty() && letters.iter().all(|c| c.is_uppercase()) {
        return replacement.to_uppercase();
    }
    if !letters.is_empty() && letters.iter().all(|c| c.is_lowercase()) {
        return replacement.to_lowercase();
    }
    if letters.first().is_some_and(|c| c.is_uppercase())
        && letters.iter().skip(1).all(|c| c.is_lowercase())
    {
        let mut chars = replacement.chars();
        return chars
            .next()
            .map_or_else(String::new, |c| c.to_uppercase().collect::<String>())
            + &chars.as_str().to_lowercase();
    }
    replacement.to_owned()
}

/// One editor edit, so Replace All is exactly one Undo, even after prior typing.
/// Selection and scroll survive an edit; owned decorations are refreshed by hosts.
pub fn apply(
    editor: &mut gpui_component::input::EditorState,
    text: String,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<gpui_component::input::EditorState>,
) {
    let selection = editor.selected_range();
    let scroll = editor.scroll_offset();
    if editor.text() == text.as_str() {
        return;
    }
    editor.replace_all(text, window, cx);
    editor.set_selected_range(selection, cx);
    editor.set_scroll_offset(scroll, cx);
}

pub fn remap_range(range: std::ops::Range<usize>, edits: &[Match]) -> std::ops::Range<usize> {
    fn offset(at: usize, edits: &[Match]) -> usize {
        let mut delta: isize = 0;
        for edit in edits {
            if at <= edit.range.start {
                break;
            }
            if at < edit.range.end {
                return edit.range.start.saturating_add_signed(delta)
                    + (at - edit.range.start).min(edit.replacement.len());
            }
            delta += edit.replacement.len() as isize - (edit.range.end - edit.range.start) as isize;
        }
        at.saturating_add_signed(delta)
    }
    offset(range.start, edits)..offset(range.end, edits)
}

pub fn apply_matches(
    editor: &mut gpui_component::input::EditorState,
    edits: &[Match],
    window: &mut gpui::Window,
    cx: &mut gpui::Context<gpui_component::input::EditorState>,
) {
    if edits.is_empty() {
        return;
    }
    let selection = remap_range(editor.selected_range(), edits);
    let scroll = editor.scroll_offset();
    let text = replaced(&editor.text().to_string(), edits);
    apply(editor, text, window, cx);
    editor.set_selected_range(selection, cx);
    editor.set_scroll_offset(scroll, cx);
}

/// Construct the complete result before making one editor transaction.
pub fn replaced(text: &str, matches: &[Match]) -> String {
    let removed: usize = matches.iter().map(|m| m.range.len()).sum();
    let inserted: usize = matches.iter().map(|m| m.replacement.len()).sum();
    let mut result = String::with_capacity(text.len() - removed + inserted);
    let mut cursor = 0;
    for found in matches {
        result.push_str(&text[cursor..found.range.start]);
        result.push_str(&found.replacement);
        cursor = found.range.end;
    }
    result.push_str(&text[cursor..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forward_replacement_keeps_dense_prefix_unicode_tail_and_zero_width_order() {
        let tail = "я🦀".repeat(100_000);
        let text = "cat".repeat(20_000) + &tail;
        let query = Query {
            text: "cat".into(),
            ..Default::default()
        };
        let edits = matches(&text, &query, "", false).unwrap();
        assert_eq!(replaced(&text, &edits), tail);
        let edits = matches(&text, &query, "elephant", false).unwrap();
        assert_eq!(replaced(&text, &edits), "elephant".repeat(20_000) + &tail);
        assert_eq!(
            replaced(
                "я",
                &[
                    Match {
                        range: 0..0,
                        replacement: "[".into()
                    },
                    Match {
                        range: 2..2,
                        replacement: "]".into()
                    },
                ]
            ),
            "[я]"
        );
    }
    #[test]
    fn cancelled_match_collection_never_returns_a_partial_edit_plan() {
        let mut checks = 0;
        let result = matches_cancellable(
            &"cat".repeat(10_000),
            &Query {
                text: "cat".into(),
                ..Default::default()
            },
            "dog",
            false,
            || {
                checks += 1;
                checks == 16
            },
        )
        .unwrap();
        assert!(result.is_none());
        assert_eq!(checks, 16);
    }

    #[test]
    fn unicode_word_regex_and_capture_replacement() {
        let query = Query {
            text: "(?P<word>кот)".into(),
            regular_expression: true,
            whole_word: true,
            ..Default::default()
        };
        let found = matches("Кот котик кот", &query, "${word}!", false).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(replaced("Кот котик кот", &found), "Кот! котик кот!");
    }
    #[test]
    fn literal_preserve_case_and_multiline() {
        let query = Query {
            text: "cat".into(),
            ..Default::default()
        };
        let found = matches("CAT Cat cat", &query, "dog", true).unwrap();
        assert_eq!(replaced("CAT Cat cat", &found), "DOG Dog dog");
        let query = Query {
            text: "a\nb".into(),
            ..Default::default()
        };
        assert_eq!(
            matches("a\nb", &query, "$1", false).unwrap()[0].replacement,
            "$1"
        );
    }
    #[test]
    fn byte_capture_replacement_cannot_make_invalid_utf8() {
        let query = Query {
            text: "(?-u:(.)(.))".into(),
            regular_expression: true,
            case_sensitive: true,
            ..Default::default()
        };
        assert!(matches("я", &query, "$1", false).is_err());
    }

    #[test]
    fn invalid_empty_and_zero_width_are_bounded() {
        assert!(
            matches("abc", &Query::default(), "x", false)
                .unwrap()
                .is_empty()
        );
        let query = Query {
            text: "(".into(),
            regular_expression: true,
            ..Default::default()
        };
        assert!(matches("abc", &query, "x", false).is_err());
        let query = Query {
            text: "^|$".into(),
            regular_expression: true,
            ..Default::default()
        };
        assert_eq!(matches("я", &query, "x", false).unwrap().len(), 2);
    }
}
