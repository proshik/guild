//! Syntax enclosures and the exact path back through selections actually visited.
use std::ops::Range;

/// A single source editor's extend/shrink chain, using UTF-8 byte ranges.
#[derive(Default)]
pub struct SelectionHistory {
    text: String,
    grammar: String,
    tree: Option<tree_sitter::Tree>,
    current: Option<Range<usize>>,
    previous: Vec<Range<usize>>,
}

impl SelectionHistory {
    /// Discard the chain after a manual gesture, navigation or edit.
    pub fn reset(&mut self) {
        self.current = None;
        self.previous.clear();
    }

    fn sync(&mut self, text: &str, current: &Range<usize>) {
        if self.text != text || self.current.as_ref() != Some(current) {
            self.reset();
        }
        if self.text != text {
            self.text = text.to_owned();
            self.tree = None;
            self.grammar.clear();
        }
    }

    /// The smallest strict enclosure offered by syntax and textual boundaries.
    pub fn extend(&mut self, text: &str, grammar: &str, current: Range<usize>) -> Range<usize> {
        self.sync(text, &current);
        if current.start > current.end
            || !text.is_char_boundary(current.start)
            || !text.is_char_boundary(current.end)
        {
            return current;
        }
        if self.grammar != grammar {
            self.grammar = grammar.to_owned();
            // Cache a cancelled snapshot as text-only instead of retrying
            // each key.
            self.tree = parse_bounded(text, grammar);
        }
        let mut ranges = textual_ranges(text, &current);
        if let Some(tree) = &self.tree {
            let mut node = tree
                .root_node()
                .descendant_for_byte_range(current.start, current.end);
            while let Some(at) = node {
                if at.is_named() && !at.is_error() {
                    ranges.push(at.byte_range());
                    // Paired delimiters belong to the syntax node, not to a
                    // scan for quotes that might actually be escaped or commented.
                    let value = &text[at.byte_range()];
                    if let Some(inner) = delimited(value) {
                        ranges.push(at.start_byte() + inner.start..at.start_byte() + inner.end);
                    }
                }
                node = at.parent();
            }
        }
        let next = ranges
            .into_iter()
            .filter(|r| {
                r.start <= current.start
                    && r.end >= current.end
                    && *r != current
                    && !r.is_empty()
                    && text.is_char_boundary(r.start)
                    && text.is_char_boundary(r.end)
            })
            .min_by_key(|r| (r.len(), r.start));
        if let Some(next) = next {
            self.previous.push(current);
            self.current = Some(next.clone());
            next
        } else {
            self.current = Some(current.clone());
            current
        }
    }

    /// Restore the preceding range, including an original caret or mouse range.
    pub fn shrink(&mut self, text: &str, current: Range<usize>) -> Range<usize> {
        self.sync(text, &current);
        let next = self.previous.pop().unwrap_or(current);
        self.current = Some(next.clone());
        next
    }
}

/// The syntax tree of `text` a keystroke may ask for: the editor's syntax
/// budget, and within a few milliseconds. Unsupported grammars, plain text and
/// a text past the highlighting limit have none — never the registry's JSON
/// fallback — and neither has a parse that ran out of time.
pub(super) fn parse_bounded(text: &str, grammar: &str) -> Option<tree_sitter::Tree> {
    if grammar == crate::highlight::PLAIN || text.len() as u64 > crate::file_tab::HIGHLIGHT_LIMIT {
        return None;
    }
    let config = crate::grammars::all()
        .into_iter()
        .find(|g| g.name == grammar)?;
    let started = std::time::Instant::now();
    let mut checkpoints = 0;
    parse_cancellable(text, &(config.language)(), || {
        checkpoints += 1;
        checkpoints > 64 || started.elapsed() >= std::time::Duration::from_millis(8)
    })
}

/// Cancellation is cooperative at tree-sitter's progress checkpoints. A fresh
/// parser is discarded on cancellation, so subsequent gestures never resume an
/// unfinished parse. Textual enclosures remain available without a syntax tree.
fn parse_cancellable(
    text: &str,
    language: &tree_sitter::Language,
    mut cancelled: impl FnMut() -> bool,
) -> Option<tree_sitter::Tree> {
    if cancelled() {
        return None;
    }
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(language).ok()?;
    let mut progress = |_: &tree_sitter::ParseState| {
        if cancelled() {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    };
    parser.parse_with_options(
        &mut |offset, _| text.as_bytes().get(offset..).unwrap_or_default(),
        None,
        Some(tree_sitter::ParseOptions::new().progress_callback(&mut progress)),
    )
}

/// Prose is independent of the grammar. Bound both work and stack space for
/// malformed/generated documents; outside this local window line/paragraph/
/// document selection still works. Do not invent a closing delimiter at EOF.
fn prose_ranges(
    text: &str,
    paragraph: Range<usize>,
    current: &Range<usize>,
    ranges: &mut Vec<Range<usize>>,
) {
    const HALF_WINDOW: usize = 32 * 1024;
    if current.len() > HALF_WINDOW * 2 {
        return;
    }
    let mut start = paragraph
        .start
        .max(current.start.saturating_sub(HALF_WINDOW));
    let mut end = paragraph.end.min(current.end.saturating_add(HALF_WINDOW));
    while !text.is_char_boundary(start) {
        start += 1;
    }
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut stack: Vec<(char, usize, usize)> = Vec::new();
    let mut pairs: Vec<Range<usize>> = Vec::new();
    let mut chars = text[start..end].char_indices().peekable();
    let mut previous = None;
    let mut escaped = false;
    while let Some((relative, ch)) = chars.next() {
        let at = start + relative;
        let was_escaped = escaped;
        escaped = !escaped && ch == '\\';
        let apostrophe = matches!(ch, '\'' | '’')
            && previous.is_some_and(unicode_ident::is_xid_continue)
            && chars
                .peek()
                .is_some_and(|(_, next)| unicode_ident::is_xid_continue(*next));
        previous = Some(ch);
        if was_escaped || apostrophe {
            continue;
        }
        if let Some(index) = stack.iter().rposition(|(close, _, _)| *close == ch) {
            let (_, open, inner) = stack[index];
            stack.truncate(index);
            ranges.push(inner..at);
            pairs.push(open..at + ch.len_utf8());
            continue;
        }
        let close = match ch {
            '"' => '"',
            '\'' => '\'',
            '`' => '`',
            '«' => '»',
            '‹' => '›',
            '„' => '“',
            '‚' => '‘',
            '“' => '”',
            '‘' => '’',
            '(' => ')',
            '[' => ']',
            '{' => '}',
            _ => continue,
        };
        if stack.len() < 64 {
            stack.push((close, at, at + ch.len_utf8()));
        }
    }
    ranges.extend(pairs.iter().cloned());
    // Merge nested enclosures for a linear sentence scan. Punctuation inside
    // quotes/brackets ends a sentence only if followed solely by closing marks.
    pairs.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
    let mut protected: Vec<Range<usize>> = Vec::new();
    for pair in pairs {
        if let Some(last) = protected.last_mut()
            && pair.start < last.end
        {
            last.end = last.end.max(pair.end);
        } else {
            protected.push(pair);
        }
    }
    let mut sentence_start = start;
    let mut known_start = start == paragraph.start;
    let mut protected_index = 0;
    for (relative, ch) in text[start..end].char_indices() {
        if !matches!(ch, '.' | '!' | '?' | '…') {
            continue;
        }
        let at = start + relative;
        let mut boundary = at + ch.len_utf8();
        while protected.get(protected_index).is_some_and(|r| r.end <= at) {
            protected_index += 1;
        }
        if let Some(pair) = protected.get(protected_index)
            && pair.start <= at
        {
            if !text[boundary..pair.end].chars().all(|ch| {
                matches!(
                    ch,
                    '"' | '\'' | '`' | '»' | '›' | '”' | '’' | '“' | '‘' | ')' | ']' | '}'
                )
            }) {
                continue;
            }
            boundary = pair.end;
        }
        if boundary < end && !text[boundary..end].starts_with(char::is_whitespace) {
            continue;
        }
        if known_start {
            push_trimmed(text, sentence_start..boundary, ranges);
        }
        sentence_start = boundary;
        known_start = true;
    }
    if known_start && end == paragraph.end {
        push_trimmed(text, sentence_start..end, ranges);
    }
}

fn push_trimmed(text: &str, range: Range<usize>, ranges: &mut Vec<Range<usize>>) {
    let value = &text[range.clone()];
    let start = range.start + value.len() - value.trim_start().len();
    let end = range.end - (value.len() - value.trim_end().len());
    if start < end {
        ranges.push(start..end);
    }
}

fn delimited(value: &str) -> Option<Range<usize>> {
    for (open, close) in [
        ("\"\"\"", "\"\"\""),
        ("'''", "'''"),
        ("\"", "\""),
        ("'", "'"),
        ("`", "`"),
        ("(", ")"),
        ("[", "]"),
        ("{", "}"),
    ] {
        if value.len() >= open.len() + close.len()
            && value.starts_with(open)
            && value.ends_with(close)
        {
            return Some(open.len()..value.len() - close.len());
        }
    }
    None
}

fn textual_ranges(text: &str, current: &Range<usize>) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    ranges.push(0..text.len());
    let word = |ch: char| unicode_ident::is_xid_continue(ch);
    let start = text[..current.start]
        .char_indices()
        .rev()
        .find(|(_, ch)| !word(*ch))
        .map_or(0, |(i, ch)| i + ch.len_utf8());
    let end = text[current.end..]
        .char_indices()
        .find(|(_, ch)| !word(*ch))
        .map_or(text.len(), |(i, _)| current.end + i);
    if text[current.clone()].chars().all(word) {
        ranges.push(start..end);
    }
    let line_start = text[..current.start].rfind('\n').map_or(0, |i| i + 1);
    // A range ending at a line boundary belongs to the preceding line.
    let last = if current.end > current.start && text[..current.end].ends_with('\n') {
        current.end - 1
    } else {
        current.end
    };
    let line_end = text[last..].find('\n').map_or(text.len(), |i| last + i);
    let content_end = if line_end > line_start && text.as_bytes()[line_end - 1] == b'\r' {
        line_end - 1
    } else {
        line_end
    };
    let line = &text[line_start..content_end];
    let trimmed = line.trim();
    let trim_start = line.len() - line.trim_start().len();
    ranges.push(line_start + trim_start..line_start + trim_start + trimmed.len());
    ranges.push(line_start..content_end);
    if line_end < text.len() {
        ranges.push(line_start..line_end + 1);
    }
    let mut paragraph_start = line_start;
    while paragraph_start > 0 {
        let end = paragraph_start - 1;
        let start = text[..end].rfind('\n').map_or(0, |i| i + 1);
        if text[start..end].trim().is_empty() {
            break;
        }
        paragraph_start = start;
    }
    let mut paragraph_end = line_end;
    while paragraph_end < text.len() {
        let start = paragraph_end + 1;
        let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
        if text[start..end].trim().is_empty() {
            break;
        }
        paragraph_end = end;
    }
    let paragraph_content_end =
        if paragraph_end > paragraph_start && text.as_bytes()[paragraph_end - 1] == b'\r' {
            paragraph_end - 1
        } else {
            paragraph_end
        };
    prose_ranges(
        text,
        paragraph_start..paragraph_content_end,
        current,
        &mut ranges,
    );
    ranges.push(paragraph_start..paragraph_content_end);
    ranges
}

use gpui::{App, AppContext, Context, Entity, Subscription};
use gpui_component::input::EditorState;

gpui::actions!(guild_editor, [ExtendSelection, ShrinkSelection]);

/// Selection history owned by one FileTab editor, including readonly sources.
/// The observer invalidates history for programmatic moves and replacements.
pub struct SmartSelection {
    history: SelectionHistory,
    snapshot: gpui_component::Rope,
    selected: Range<usize>,
    cursor: usize,
    _observe: Subscription,
}

impl SmartSelection {
    pub fn attach(editor: &Entity<EditorState>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| Self {
            history: SelectionHistory::default(),
            snapshot: editor.read(cx).text().clone(),
            selected: editor.read(cx).selected_range(),
            cursor: editor.read(cx).cursor(),
            _observe: cx.observe(editor, |this, editor, cx| {
                let state = editor.read(cx);
                if !ropey::extra::esoterica::ropes_are_instances(&this.snapshot, state.text())
                    || this.selected != state.selected_range()
                    || this.cursor != state.cursor()
                {
                    this.history.reset();
                    this.snapshot = state.text().clone();
                    this.selected = state.selected_range();
                    this.cursor = state.cursor();
                }
            }),
        })
    }

    pub fn reset(&mut self) {
        self.history.reset();
    }

    pub fn select(&mut self, editor: &Entity<EditorState>, extend: bool, cx: &mut Context<Self>) {
        let state = editor.read(cx);
        let text = state.value();
        let current = state.selected_range();
        let next = if extend {
            self.history
                .extend(&text, &state.language_name(), current.clone())
        } else {
            self.history.shrink(&text, current.clone())
        };
        // A no-op shrink must also leave additional cursors untouched.
        if next != current {
            editor.update(cx, |state, cx| state.set_selected_range(next, cx));
        }
        let state = editor.read(cx);
        self.snapshot = state.text().clone();
        self.selected = state.selected_range();
        self.cursor = state.cursor();
    }
}

pub(crate) fn bindings() -> Vec<gpui::KeyBinding> {
    let context = Some(
        "((guild-file || guild-readonly-file) > Input) && !guild-field && !guild-local-search && !recording",
    );
    vec![
        gpui::KeyBinding::new("cmd-up", ExtendSelection, context),
        gpui::KeyBinding::new("cmd-down", ShrinkSelection, context),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn parsing_cancels_at_progress_and_textual_selection_survives() {
        let source = format!(
            "fn main() {{ {} }}",
            "let text = \"hello world\";".repeat(20_000)
        );
        let mut calls = 0;
        let tree = super::parse_cancellable(&source, &tree_sitter_rust::LANGUAGE.into(), || {
            calls += 1;
            calls > 1
        });
        assert!(tree.is_none());
        assert!(
            calls >= 2,
            "exercise cancellation inside tree-sitter, after the initial budget check"
        );
        let mut history = super::SelectionHistory {
            text: source.clone(),
            grammar: "rust".into(),
            tree,
            ..Default::default()
        };
        let word = source.find("world").unwrap();
        let selected = history.extend(&source, "rust", word..word + 5);
        assert_eq!(&source[selected.clone()], "hello world");
        assert_eq!(history.shrink(&source, selected), word..word + 5);
        assert!(
            super::parse_cancellable("fn f() {}", &tree_sitter_rust::LANGUAGE.into(), || false)
                .is_some()
        );
        assert!(
            super::parse_cancellable("fn f() {}", &tree_sitter_rust::LANGUAGE.into(), || true)
                .is_none()
        );
    }

    // The keymap is private adapter wiring; public integration tests exercise
    // the production FileTab dispatch and active selection.
    #[gpui::test]
    fn overrides_are_scoped_and_survive_rebinding(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::editor::init(cx);
            crate::actions::install(cx);
            for (stroke, expected) in [
                ("cmd-up", "guild_editor::ExtendSelection"),
                ("cmd-down", "guild_editor::ShrinkSelection"),
            ] {
                let keys = [gpui::Keystroke::parse(stroke).unwrap()];
                for host in ["guild-file", "guild-readonly-file"] {
                    for field in [None, Some("guild-field"), Some("guild-local-search")] {
                        let mut contexts = vec!["Root", "guild", host];
                        contexts.extend(field);
                        contexts.push("Input");
                        let contexts = contexts
                            .into_iter()
                            .map(|c| gpui::KeyContext::parse(c).unwrap())
                            .collect::<Vec<_>>();
                        let found = cx
                            .key_bindings()
                            .borrow()
                            .bindings_for_input(&keys, &contexts)
                            .0;
                        let name = found.first().unwrap().action().name();
                        assert_eq!(
                            name == expected,
                            field.is_none(),
                            "{host} {field:?} {stroke}: {name}"
                        );
                    }
                }
                for host in ["terminal", "guild-field", "changes"] {
                    let contexts = ["Root", "guild", host, "Input"]
                        .into_iter()
                        .map(|c| gpui::KeyContext::parse(c).unwrap())
                        .collect::<Vec<_>>();
                    let found = cx
                        .key_bindings()
                        .borrow()
                        .bindings_for_input(&keys, &contexts)
                        .0;
                    assert!(found.iter().all(|b| b.action().name() != expected));
                }
            }
        });
    }
}
