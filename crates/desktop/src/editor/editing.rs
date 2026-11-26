//! Line and comment editing, as pure functions over a text.
//!
//! Every function takes the text, the selection and the caret in UTF-8
//! bytes and answers **one** replacement — so an action is one undo step —
//! with the selection after it, or `None` when there is nothing to do.
//! Columns are counted in characters, so a Cyrillic line keeps its caret
//! where a hand put it. A `\r` before a `\n` is the line's ending, never its
//! content: a buffer of a CRLF file is LF in the editor, but a stray CR
//! must not be trimmed into the next line or counted as a column.
use std::ops::Range;

use super::comments::Comment;

/// One replacement, and the selection after it in the new text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
}

impl Edit {
    /// The text with this replacement made.
    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_owned();
        out.replace_range(self.range.clone(), &self.text);
        out
    }
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

/// The `\n` ending the line `at` is on, or the end of the text.
fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |i| at + i)
}

/// Where the line's content stops: before a `\r\n`, or at its `end`.
fn content_end(text: &str, start: usize, end: usize) -> usize {
    if end > start && text.as_bytes()[end - 1] == b'\r' && end < text.len() {
        end - 1
    } else {
        end
    }
}

/// Past the line's ending: after its `\n`, or the end of the text.
fn terminated(text: &str, end: usize) -> usize {
    if end < text.len() { end + 1 } else { end }
}

/// The lines a selection touches: from the first one's start to the `\n` (or
/// the end) of the last one. A selection ending at a line's very start does
/// not touch that line — the common IDE rule, and what a triple click leaves.
fn block(text: &str, selection: &Range<usize>) -> (usize, usize) {
    let last =
        if selection.end > selection.start && line_start(text, selection.end) == selection.end {
            selection.end - 1
        } else {
            selection.end
        };
    (line_start(text, selection.start), line_end(text, last))
}

fn column(text: &str, at: usize) -> usize {
    text[line_start(text, at)..at].chars().count()
}

/// The offset of `column` characters into the line starting at `start`,
/// clamped to the line's content.
fn at_column(text: &str, start: usize, column: usize) -> usize {
    let end = content_end(text, start, line_end(text, start));
    text[start..end]
        .char_indices()
        .nth(column)
        .map_or(end, |(i, _)| start + i)
}

/// The newline a new line should end with: the file's, as the line beside it
/// shows it.
fn newline(text: &str, start: usize, end: usize) -> &'static str {
    let crlf = content_end(text, start, end) < end || text.as_bytes()[..start].ends_with(b"\r\n");
    if crlf { "\r\n" } else { "\n" }
}

fn indentation(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// **Delete Line**: every line the selection touches. The caret keeps its
/// column on the line that takes their place; the last line takes the
/// newline before it.
pub fn delete_lines(text: &str, selection: Range<usize>, caret: usize) -> Option<Edit> {
    if text.is_empty() {
        return None;
    }
    let (start, end) = block(text, &selection);
    let column = column(text, caret);
    if end < text.len() {
        let next = end + 1;
        let caret = at_column(text, next, column) - (next - start);
        return Some(Edit {
            range: start..next,
            text: String::new(),
            selection: caret..caret,
        });
    }
    if start == 0 {
        return Some(Edit {
            range: 0..text.len(),
            text: String::new(),
            selection: 0..0,
        });
    }
    let from = if text[..start].ends_with("\r\n") {
        start - 2
    } else {
        start - 1
    };
    let caret = at_column(text, line_start(text, start - 1), column);
    Some(Edit {
        range: from..end,
        text: String::new(),
        selection: caret..caret,
    })
}

/// **Duplicate**: the caret's line below itself, the caret moving into the
/// copy; a selection, right after itself, the copy selected.
pub fn duplicate(text: &str, selection: Range<usize>, caret: usize) -> Option<Edit> {
    if !selection.is_empty() {
        let copy = text[selection.clone()].to_owned();
        let len = copy.len();
        return Some(Edit {
            range: selection.end..selection.end,
            text: copy,
            selection: selection.end..selection.end + len,
        });
    }
    let start = line_start(text, caret);
    let end = line_end(text, caret);
    let (at, copy) = if end < text.len() {
        (end + 1, text[start..=end].to_owned())
    } else {
        (
            end,
            format!("{}{}", newline(text, start, end), &text[start..end]),
        )
    };
    let caret = caret + copy.len();
    Some(Edit {
        range: at..at,
        text: copy,
        selection: caret..caret,
    })
}

/// **Move Line Up/Down**: the lines the selection touches, past their
/// neighbour; the selection moves with them. Nothing at the file's edges.
pub fn move_lines(text: &str, selection: Range<usize>, _caret: usize, up: bool) -> Option<Edit> {
    let (start, end) = block(text, &selection);
    let block_content = content_end(text, start, end);
    let block_end = terminated(text, end);
    let (range, moved, delta) = if up {
        if start == 0 {
            return None;
        }
        let previous = line_start(text, start - 1);
        let previous_content = content_end(text, previous, start - 1);
        let moved = [
            &text[start..block_content],
            &text[previous_content..start],
            &text[previous..previous_content],
            &text[block_content..block_end],
        ]
        .concat();
        (previous..block_end, moved, -((start - previous) as isize))
    } else {
        if block_end == block_content {
            return None;
        }
        let next = block_end;
        let next_end = line_end(text, next);
        let next_content = content_end(text, next, next_end);
        let moved = [
            &text[next..next_content],
            &text[block_content..block_end],
            &text[start..block_content],
            &text[next_content..terminated(text, next_end)],
        ]
        .concat();
        let delta = (next_content - next) + (block_end - block_content);
        (start..terminated(text, next_end), moved, delta as isize)
    };
    let shift = |at: usize| (at as isize + delta) as usize;
    Some(Edit {
        range,
        text: moved,
        selection: shift(selection.start)..shift(selection.end),
    })
}

/// Whether a line joined onto another needs no space before it.
fn joins_tight(next: &str) -> bool {
    next.is_empty() || next.starts_with([')', ']', '}', ',', '.', ';'])
}

/// **Join Lines**: the next line onto the caret's, or every line of a
/// selection into one. The joined line's leading and the current line's
/// trailing whitespace go, and one space separates them where it reads.
/// The caret stands where the last two met.
pub fn join_lines(text: &str, selection: Range<usize>, _caret: usize) -> Option<Edit> {
    let (start, mut end) = block(text, &selection);
    if !text[start..end].contains('\n') {
        if end == text.len() {
            return None;
        }
        end = line_end(text, end + 1);
    }
    let end = content_end(text, line_start(text, end), end);
    let mut lines = text[start..end].split('\n');
    let mut joined = lines.next().unwrap_or_default().to_owned();
    let mut at = 0;
    for line in lines {
        joined.truncate(joined.trim_end().len());
        at = joined.len();
        let line = line.trim_start();
        // `joined` is trimmed at its end, so a blank current line is empty.
        if !(joins_tight(line) || joined.is_empty()) {
            joined.push(' ');
        }
        joined.push_str(line);
    }
    let caret = start + at;
    Some(Edit {
        range: start..end,
        text: joined,
        selection: caret..caret,
    })
}

/// Edits inside one span, sorted and disjoint: (offset, deleted, inserted).
type Splices = Vec<(usize, usize, String)>;

/// Where `at` lands after `splices`. `after` puts an offset at an insertion
/// point behind what was inserted, so a selection grows to take it in.
fn map(splices: &Splices, at: usize, after: bool) -> usize {
    let mut shift = 0isize;
    for (offset, deleted, inserted) in splices {
        if at < *offset || (at == *offset && !after) {
            break;
        }
        if at >= offset + deleted {
            shift += inserted.len() as isize - *deleted as isize;
        } else {
            let base = (*offset as isize + shift) as usize;
            return if after { base + inserted.len() } else { base };
        }
    }
    (at as isize + shift) as usize
}

fn spliced(text: &str, range: Range<usize>, splices: &Splices) -> String {
    let mut out = String::with_capacity(range.len());
    let mut from = range.start;
    for (offset, deleted, inserted) in splices {
        out.push_str(&text[from..*offset]);
        out.push_str(inserted);
        from = offset + deleted;
    }
    out.push_str(&text[from..range.end]);
    out
}

/// **Comment with Line Comment**: toggles the language's line comment on
/// every line touched. When every non-blank line is commented the token
/// goes, with one space after it; otherwise every non-blank line gets
/// `token ` at the block's smallest indentation. A language with only a
/// block comment wraps each line instead. With no selection the caret steps
/// to the next line, as IDEs commonly do.
pub fn toggle_line_comment(
    text: &str,
    selection: Range<usize>,
    caret: usize,
    comment: Comment,
) -> Option<Edit> {
    let (start, end) = block(text, &selection);
    let end = content_end(text, line_start(text, end), end);
    let mut lines = Vec::new();
    let mut offset = start;
    for line in text[start..end].split('\n') {
        let content = line.strip_suffix('\r').unwrap_or(line);
        if !content.trim().is_empty() {
            lines.push((offset, content));
        }
        offset += line.len() + 1;
    }
    if lines.is_empty() {
        return None;
    }
    let mut splices: Splices = Vec::new();
    if let Some(token) = comment.line {
        let commented = lines
            .iter()
            .all(|(_, line)| line.trim_start().starts_with(token));
        if commented {
            for (offset, line) in &lines {
                let at = offset + indentation(line).len();
                let space = line[at - offset + token.len()..].starts_with(' ');
                splices.push((at, token.len() + usize::from(space), String::new()));
            }
        } else {
            let indent = lines
                .iter()
                .map(|(_, line)| indentation(line).chars().count())
                .min()
                .unwrap_or(0);
            for (offset, line) in &lines {
                let at = offset
                    + line
                        .char_indices()
                        .nth(indent)
                        .map_or(line.len(), |(i, _)| i);
                splices.push((at, 0, format!("{token} ")));
            }
        }
    } else if let Some((open, close)) = comment.block {
        let wrapped = |line: &str| {
            let line = line.trim();
            line.len() >= open.len() + close.len()
                && line.starts_with(open)
                && line.ends_with(close)
        };
        if lines.iter().all(|(_, line)| wrapped(line)) {
            for (offset, line) in &lines {
                let at = offset + indentation(line).len();
                let space = line[at - offset + open.len()..].starts_with(' ');
                let opened = at + open.len() + usize::from(space);
                splices.push((at, opened - at, String::new()));
                // `<!-- -->` shares its one space between the two tokens.
                let tail = (offset + line.trim_end().len() - close.len()).max(opened);
                let space = tail > opened && text[..tail].ends_with(' ');
                let from = tail - usize::from(space);
                splices.push((from, tail + close.len() - from, String::new()));
            }
        } else {
            let indent = lines
                .iter()
                .map(|(_, line)| indentation(line).chars().count())
                .min()
                .unwrap_or(0);
            for (offset, line) in &lines {
                let at = offset
                    + line
                        .char_indices()
                        .nth(indent)
                        .map_or(line.len(), |(i, _)| i);
                splices.push((at, 0, format!("{open} ")));
                splices.push((offset + line.len(), 0, format!(" {close}")));
            }
        }
    } else {
        return None;
    }
    let new = spliced(text, start..end, &splices);
    let delta = new.len() as isize - (end - start) as isize;
    let selection = if selection.is_empty() {
        let next = terminated(text, line_end(text, end));
        let caret = if next > line_end(text, end) {
            (at_column(text, next, column(text, caret)) as isize + delta) as usize
        } else {
            map(&splices, caret, true)
        };
        caret..caret
    } else {
        map(&splices, selection.start, false)..map(&splices, selection.end, true)
    };
    Some(Edit {
        range: start..end,
        text: new,
        selection,
    })
}

/// What the file's syntax says of a selection and the comments around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Around {
    /// No tree to ask: plain text, a file past the highlighting limit, a
    /// grammar with no comment node, a parse out of time.
    Unknown,
    /// The tree says no block comment holds the selection.
    Nothing,
    /// The block comment holding the selection, delimiters included.
    Comment(Range<usize>),
}

/// The selection without the whitespace at its two ends.
fn trimmed(text: &str, selection: &Range<usize>) -> Range<usize> {
    let selected = &text[selection.clone()];
    let start = selection.start + selected.len() - selected.trim_start().len();
    let end = selection.end - (selected.len() - selected.trim_end().len());
    start..end.max(start)
}

/// **Comment with Block Comment**: a selection is wrapped in the language's
/// block tokens, or unwrapped when it is one block comment or lies inside
/// one; without a selection an empty pair goes in with the caret between.
///
/// **One comment is the grammar's word, not a search's.** With a tree,
/// `around` decides: tokens found by scanning cannot tell a comment from a
/// string holding `"/*"`, nor a nested comment from two with code between.
/// Only without one is an exact selection asked of its text, and then two
/// comments are never taken for one.
pub fn toggle_block_comment(
    text: &str,
    selection: Range<usize>,
    caret: usize,
    comment: Comment,
    around: Around,
) -> Option<Edit> {
    let (open, close) = comment.block?;
    if selection.is_empty() {
        let at = open.len() + caret;
        return Some(Edit {
            range: caret..caret,
            text: format!("{open}{close}"),
            selection: at..at,
        });
    }
    let unwrap = match around {
        Around::Comment(range) => Some(range),
        Around::Nothing => None,
        Around::Unknown => {
            let range = trimmed(text, &selection);
            let value = &text[range.clone()];
            (value.len() >= open.len() + close.len()
                && value.starts_with(open)
                && value.ends_with(close)
                && !value[open.len()..value.len() - close.len()].contains(close))
            .then_some(range)
        }
    };
    if let Some(range) = unwrap {
        let inner = range.start + open.len()..range.end - close.len();
        let map = |at: usize| {
            if at <= range.start {
                at
            } else if at >= range.end {
                at - open.len() - close.len()
            } else {
                at.clamp(inner.start, inner.end) - open.len()
            }
        };
        return Some(Edit {
            range: range.clone(),
            text: text[inner.clone()].to_owned(),
            selection: map(selection.start)..map(selection.end),
        });
    }
    let selected = &text[selection.clone()];
    let new = format!("{open}{selected}{close}");
    let len = new.len();
    Some(Edit {
        range: selection.clone(),
        text: new,
        selection: selection.start..selection.start + len,
    })
}

/// The block comment of `grammar`'s syntax that holds `selection` — its
/// ends trimmed of whitespace — or is it. The comment is the first ancestor
/// of one of the grammar's comment kinds that carries the delimiters: a
/// Rust `/** */`'s `doc_comment` and Lua's `comment_content` sit inside it
/// without them. Parsed under the same budget as smart selection; plain
/// text, a file past the highlighting limit, a grammar whose tree has no
/// comment node or a parse out of time give [`Around::Unknown`].
pub fn comment_around(text: &str, grammar: &str, selection: Range<usize>) -> Around {
    let comment = super::comments::for_grammar(grammar);
    let Some((open, close)) = comment.block else {
        return Around::Unknown;
    };
    if comment.nodes.is_empty() {
        return Around::Unknown;
    }
    let Some(tree) = super::selection::parse_bounded(text, grammar) else {
        return Around::Unknown;
    };
    let selection = trimmed(text, &selection);
    let mut node = tree
        .root_node()
        .descendant_for_byte_range(selection.start, selection.end);
    while let Some(at) = node {
        let range = at.byte_range();
        let value = &text[range.clone()];
        if comment.nodes.contains(&at.kind())
            && range.start <= selection.start
            && selection.end <= range.end
            && value.len() >= open.len() + close.len()
            && value.starts_with(open)
            && value.ends_with(close)
        {
            return Around::Comment(range);
        }
        node = at.parent();
    }
    Around::Nothing
}

/// **Start New Line** below the caret's line, or above it: the line is not
/// split, the new one takes its indentation and the caret goes to its end.
pub fn start_new_line(
    text: &str,
    _selection: Range<usize>,
    caret: usize,
    above: bool,
) -> Option<Edit> {
    let start = line_start(text, caret);
    let end = line_end(text, caret);
    let content = content_end(text, start, end);
    let indent = indentation(&text[start..content]).to_owned();
    let newline = newline(text, start, end);
    if above {
        let caret = start + indent.len();
        Some(Edit {
            range: start..start,
            text: format!("{indent}{newline}"),
            selection: caret..caret,
        })
    } else {
        let caret = content + newline.len() + indent.len();
        Some(Edit {
            range: content..content,
            text: format!("{newline}{indent}"),
            selection: caret..caret,
        })
    }
}

/// The identifier around `at`, empty when there is none.
fn word(text: &str, at: usize) -> Range<usize> {
    let word = |ch: char| unicode_ident::is_xid_continue(ch);
    let start = text[..at]
        .char_indices()
        .rev()
        .find(|(_, ch)| !word(*ch))
        .map_or(0, |(i, ch)| i + ch.len_utf8());
    let end = text[at..]
        .char_indices()
        .find(|(_, ch)| !word(*ch))
        .map_or(text.len(), |(i, _)| at + i);
    start..end
}

/// **Toggle Case**: the selection, or the word under the caret, upper case
/// when it holds any lower-case letter and lower case otherwise.
pub fn toggle_case(text: &str, selection: Range<usize>, caret: usize) -> Option<Edit> {
    let range = if selection.is_empty() {
        word(text, caret)
    } else {
        selection.clone()
    };
    let value = &text[range.clone()];
    let new = if value.chars().any(char::is_lowercase) {
        value.to_uppercase()
    } else {
        value.to_lowercase()
    };
    if new == value {
        return None;
    }
    let selection = if selection.is_empty() {
        // `ß` is `SS`: the caret keeps its character count into the word.
        let before = value[..caret - range.start].chars().count();
        let at = range.start + new.char_indices().nth(before).map_or(new.len(), |(i, _)| i);
        at..at
    } else {
        range.start..range.start + new.len()
    };
    Some(Edit {
        range,
        text: new,
        selection,
    })
}

use gpui::{App, Entity, Window};
use gpui_component::Rope;
use gpui_component::input::EditorState;

gpui::actions!(
    guild_editor,
    [
        DeleteLine,
        Duplicate,
        MoveLineUp,
        MoveLineDown,
        JoinLines,
        CommentWithLineComment,
        CommentWithBlockComment,
        StartNewLine,
        StartNewLineBeforeCurrent,
        ToggleCase,
    ]
);

/// Which of the editing actions to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    DeleteLine,
    Duplicate,
    MoveLine { up: bool },
    JoinLines,
    LineComment,
    BlockComment,
    StartNewLine { above: bool },
    ToggleCase,
}

/// Run `op` on an editable editor, as one replacement and so one undo step,
/// and note in `journal` the selections undo and redo should give back.
///
/// **The active selection only.** The component's public surface reads and
/// sets one selection — `selected_range` and `set_selected_range`, which
/// drops every other cursor — and has no door to the rest, so with several
/// cursors this edits at the active one and leaves it the only one.
pub fn apply(
    editor: &Entity<EditorState>,
    op: Op,
    grammar: &str,
    journal: &mut Journal,
    window: &mut Window,
    cx: &mut App,
) {
    let state = editor.read(cx);
    if !state.is_editable() {
        return;
    }
    journal.follow(editor, cx);
    let before = editor.read(cx).text().clone();
    let state = editor.read(cx);
    let text = state.value();
    let selection = state.selected_range();
    let caret = state.cursor();
    let comment = super::comments::for_grammar(grammar);
    let edit = match op {
        Op::DeleteLine => delete_lines(&text, selection.clone(), caret),
        Op::Duplicate => duplicate(&text, selection.clone(), caret),
        Op::MoveLine { up } => move_lines(&text, selection.clone(), caret, up),
        Op::JoinLines => join_lines(&text, selection.clone(), caret),
        Op::LineComment => toggle_line_comment(&text, selection.clone(), caret, comment),
        Op::BlockComment => {
            let around = comment_around(&text, grammar, selection.clone());
            toggle_block_comment(&text, selection.clone(), caret, comment, around)
        }
        Op::StartNewLine { above } => start_new_line(&text, selection.clone(), caret, above),
        Op::ToggleCase => toggle_case(&text, selection.clone(), caret),
    };
    let Some(edit) = edit else {
        return;
    };
    editor.update(cx, |state, cx| {
        state.set_selected_range(edit.range.clone(), cx);
        state.replace(edit.text, window, cx);
        state.set_selected_range(edit.selection, cx);
    });
    let state = editor.read(cx);
    journal.push(Entry {
        before,
        before_selection: selection,
        after: state.text().clone(),
        after_selection: state.selected_range(),
    });
    journal.current = Some(editor.read(cx).text().clone());
}

/// The texts on either side of one action, and the selection on each.
struct Entry {
    before: Rope,
    before_selection: Range<usize>,
    after: Rope,
    after_selection: Range<usize>,
}

/// The selections undo and redo should give back after the editing actions
/// of one buffer.
///
/// **Why a journal beside the component's history.** The component records
/// a replacement's "before" selection as whatever is selected when it
/// starts and its "after" as a caret at the end of the inserted text; its
/// public `replace` replaces the *selection*, and nothing public sets either
/// snapshot. So undo would select the helper range.
///
/// **It mirrors the history's position rather than searching it.** Actions
/// push on the undo side and drop the redo side, as the component does; an
/// undo moves the top entry across and a redo moves it back, and nothing
/// below the top is ever looked at. The journal is a faithful copy of the
/// top of the component's stacks only while every change to the buffer is
/// one of its own, so **any other change ends it** — typing, a paste, a
/// reload, another editor — and the component's own selections apply from
/// there on. It knows its own changes by the buffer's rope: what it last
/// left is kept, and a buffer that is no longer an instance of that rope
/// was changed by someone else — a check that costs nothing, where a
/// comparison of texts would cost the file's size.
#[derive(Default)]
pub struct Journal {
    editor: Option<gpui::EntityId>,
    undo: std::collections::VecDeque<Entry>,
    redo: Vec<Entry>,
    /// The buffer as the journal last left it.
    current: Option<Rope>,
    /// An undo or redo the component is running, whose outcome is read next.
    replaying: bool,
}

impl Journal {
    /// The component's own undo depth, `MAX_UNDO_TRANSACTIONS` in
    /// gpui-base 0.7.1: an entry is dropped when its transaction is. Were it
    /// dropped earlier, undoing that far would find the journal empty and
    /// leave the component's selection — never another entry's.
    const DEPTH: usize = 1000;

    fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.current = None;
    }

    /// Whether `editor` is still as the journal left it.
    fn in_step(&self, editor: &Entity<EditorState>, cx: &App) -> bool {
        self.editor == Some(editor.entity_id())
            && self.current.as_ref().is_some_and(|current| {
                ropey::extra::esoterica::ropes_are_instances(current, editor.read(cx).text())
            })
    }

    /// Before an action: start over unless the buffer is as it was left.
    fn follow(&mut self, editor: &Entity<EditorState>, cx: &App) {
        if !self.in_step(editor, cx) {
            self.clear();
            self.editor = Some(editor.entity_id());
        }
    }

    fn push(&mut self, entry: Entry) {
        if self.undo.len() == Self::DEPTH {
            self.undo.pop_front();
        }
        self.undo.push_back(entry);
        self.redo.clear();
    }

    /// The buffer changed. Unless the change is the journal's own — an
    /// action it has noted or an undo or redo it is following — it ends.
    pub fn changed(&mut self, editor: &Entity<EditorState>, cx: &App) {
        if !self.replaying && !self.in_step(editor, cx) {
            self.clear();
        }
    }

    /// An undo (or a redo) of `editor` is about to run: whether the journal
    /// has the entry it will replay, and so wants to see what it did.
    pub fn replaying(&mut self, editor: &Entity<EditorState>, undo: bool, cx: &App) -> bool {
        if !self.in_step(editor, cx) {
            self.clear();
            return false;
        }
        self.replaying = if undo {
            !self.undo.is_empty()
        } else {
            !self.redo.is_empty()
        };
        self.replaying
    }

    /// The undo (or redo) has run. If it took the buffer to the other side
    /// of the top entry, select what that side had and move the entry
    /// across; anything else means the two histories parted, and the
    /// journal ends.
    pub fn replayed(&mut self, editor: &Entity<EditorState>, undo: bool, cx: &mut App) {
        if !std::mem::take(&mut self.replaying) {
            return;
        }
        let entry = if undo {
            self.undo.pop_back()
        } else {
            self.redo.pop()
        };
        let Some(entry) = entry else {
            return;
        };
        let now = editor.read(cx).text().clone();
        let (expected, selection) = if undo {
            (&entry.before, entry.before_selection.clone())
        } else {
            (&entry.after, entry.after_selection.clone())
        };
        if now.len() != expected.len() || now != *expected {
            self.clear();
            return;
        }
        editor.update(cx, |state, cx| state.set_selected_range(selection, cx));
        self.current = Some(editor.read(cx).text().clone());
        if undo {
            self.redo.push(entry);
        } else {
            self.undo.push_back(entry);
        }
    }
}

/// The strokes, the common macOS IDE keymap's. Bound in an editable file's
/// `Input` only — never a read-only source, a single-line field, the Find bar
/// or the commit box — and after the component's own, so `cmd-backspace` and
/// `shift-enter` are ours there and the component's everywhere else.
pub(crate) fn bindings() -> Vec<gpui::KeyBinding> {
    let context = Some("guild-file > Input && !guild-field && !guild-local-search && !recording");
    vec![
        gpui::KeyBinding::new("cmd-backspace", DeleteLine, context),
        gpui::KeyBinding::new("cmd-d", Duplicate, context),
        gpui::KeyBinding::new("alt-shift-up", MoveLineUp, context),
        gpui::KeyBinding::new("alt-shift-down", MoveLineDown, context),
        gpui::KeyBinding::new("ctrl-shift-j", JoinLines, context),
        gpui::KeyBinding::new("cmd-/", CommentWithLineComment, context),
        gpui::KeyBinding::new("cmd-alt-/", CommentWithBlockComment, context),
        gpui::KeyBinding::new("shift-enter", StartNewLine, context),
        gpui::KeyBinding::new("cmd-alt-enter", StartNewLineBeforeCurrent, context),
        // The common macOS stroke, `cmd-shift-u`, is Go to Waiting Session
        // here. The common Windows and Linux one is this one, free in the
        // window, the component and macOS alike.
        gpui::KeyBinding::new("ctrl-shift-u", ToggleCase, context),
    ]
}
