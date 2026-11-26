//! Source-backed display mapping for a read-only comparison.

use std::{collections::HashSet, ops::Range, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub text: Arc<str>,
    pub lines: Vec<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayRow {
    Text {
        old: Option<usize>,
        new: Option<usize>,
        block: Option<usize>,
    },
    Fold {
        equal: usize,
        count: usize,
    },
    NoNewline {
        side: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub sources: [Source; 2],
    pub blocks: Vec<Block>,
    pub equals: Vec<Block>,
}

impl Source {
    pub(crate) fn new(text: &str) -> Self {
        let mut lines = Vec::new();
        let mut start = 0;
        let mut at = 0;
        let bytes = text.as_bytes();
        while at < bytes.len() {
            if matches!(bytes[at], b'\r' | b'\n') {
                lines.push(start..at);
                let cr = bytes[at] == b'\r';
                at += 1;
                if cr && bytes.get(at) == Some(&b'\n') {
                    at += 1;
                }
                start = at;
            } else {
                at += 1;
            }
        }
        if start < bytes.len() {
            lines.push(start..bytes.len());
        }
        Self {
            text: Arc::from(text),
            lines,
        }
    }

    pub fn line(&self, number: usize) -> &str {
        self.lines
            .get(number)
            .and_then(|range| self.text.get(range.clone()))
            .unwrap_or("")
    }
}

impl Document {
    pub fn new(old: &str, new: &str) -> Self {
        Self::with_deadline(
            old,
            new,
            std::time::Instant::now() + std::time::Duration::from_millis(150),
        )
    }

    pub(crate) fn with_deadline(old: &str, new: &str, deadline: std::time::Instant) -> Self {
        #[cfg(test)]
        super::note_computation(1);
        let sources = [Source::new(old), Source::new(new)];
        // Tokenize once from the retained source ranges. Re-tokenizing a
        // normalized string can merge a lone CR followed by a CRLF and invent
        // indices that do not exist in the source. Preserve missing final EOL.
        let keys = sources.each_ref().map(|source| {
            source
                .lines
                .iter()
                .map(|range| (&source.text[range.clone()], range.end < source.text.len()))
                .collect::<Vec<_>>()
        });
        let ops = similar::capture_diff_slices_deadline(
            similar::Algorithm::Patience,
            &keys[0],
            &keys[1],
            Some(deadline),
        );
        let mut blocks = Vec::new();
        let mut equals = Vec::new();
        for op in &ops {
            let block = Block {
                old: op.old_range(),
                new: op.new_range(),
            };
            if op.tag() == similar::DiffTag::Equal {
                equals.push(block);
            } else if let Some(last) = blocks.last_mut().filter(|last: &&mut Block| {
                last.old.end == block.old.start && last.new.end == block.new.start
            }) {
                last.old.end = block.old.end;
                last.new.end = block.new.end;
            } else {
                blocks.push(block);
            }
        }
        Self {
            sources,
            blocks,
            equals,
        }
    }

    /// Map a historical old line to the corresponding newer source location.
    /// A deletion targets its following insertion point, clamped to the file.
    pub fn new_line_for_old(&self, old: usize) -> usize {
        let mapped =
            self.equals
                .iter()
                .find(|b| b.old.contains(&old))
                .map(|b| b.new.start + old - b.old.start)
                .or_else(|| {
                    self.blocks.iter().find(|b| b.old.contains(&old)).map(|b| {
                        b.new.start + (old - b.old.start).min(b.new.len().saturating_sub(1))
                    })
                })
                .unwrap_or(old);
        mapped.min(self.sources[1].lines.len().saturating_sub(1))
    }

    pub fn rows(&self, unified: bool, expanded: &HashSet<usize>) -> Vec<DisplayRow> {
        let mut segments: Vec<(usize, usize, bool, usize)> = self
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.old.start, b.new.start, false, i))
            .chain(
                self.equals
                    .iter()
                    .enumerate()
                    .map(|(i, b)| (b.old.start, b.new.start, true, i)),
            )
            .collect();
        segments.sort_by_key(|(a, b, _, _)| (*a, *b));
        let mut rows = Vec::new();
        for (_, _, equal, index) in segments {
            let block = if equal {
                &self.equals[index]
            } else {
                &self.blocks[index]
            };
            if equal {
                let len = block.old.len();
                let leading = if block.old.start == 0 && block.new.start == 0 {
                    0
                } else {
                    3
                };
                let trailing = if block.old.end == self.sources[0].lines.len()
                    && block.new.end == self.sources[1].lines.len()
                {
                    0
                } else {
                    3
                };
                let folded = !expanded.contains(&index) && len > leading + trailing;
                for n in 0..len {
                    if folded && n >= leading && n < len - trailing {
                        if n == leading {
                            rows.push(DisplayRow::Fold {
                                equal: index,
                                count: len - leading - trailing,
                            });
                        }
                        continue;
                    }
                    rows.push(DisplayRow::Text {
                        old: Some(block.old.start + n),
                        new: Some(block.new.start + n),
                        block: None,
                    });
                }
            } else if unified {
                rows.extend(block.old.clone().map(|old| DisplayRow::Text {
                    old: Some(old),
                    new: None,
                    block: Some(index),
                }));
                rows.extend(block.new.clone().map(|new| DisplayRow::Text {
                    old: None,
                    new: Some(new),
                    block: Some(index),
                }));
            } else {
                for n in 0..block.old.len().max(block.new.len()) {
                    rows.push(DisplayRow::Text {
                        old: (n < block.old.len()).then_some(block.old.start + n),
                        new: (n < block.new.len()).then_some(block.new.start + n),
                        block: Some(index),
                    });
                }
            }
        }
        for side in 0..2 {
            let text = &self.sources[side].text;
            if !text.is_empty() && !text.ends_with('\n') && !text.ends_with('\r') {
                rows.push(DisplayRow::NoNewline { side });
            }
        }
        rows
    }
}

/// Character-safe byte spans; refinement is bounded and never changes the text.
pub fn intraline(old: &str, new: &str) -> [Vec<Range<usize>>; 2] {
    if old.len() + new.len() > 16_384 {
        return [Vec::new(), Vec::new()];
    }
    let diff = similar::TextDiff::configure()
        .timeout(std::time::Duration::from_millis(5))
        .diff_chars(old, new);
    let mut offsets = [0, 0];
    let mut result = [Vec::new(), Vec::new()];
    for change in diff.iter_all_changes() {
        let len = change.value().len();
        match change.tag() {
            similar::ChangeTag::Equal => {
                offsets[0] += len;
                offsets[1] += len;
            }
            similar::ChangeTag::Delete => {
                result[0].push(offsets[0]..offsets[0] + len);
                offsets[0] += len;
            }
            similar::ChangeTag::Insert => {
                result[1].push(offsets[1]..offsets[1] + len);
                offsets[1] += len;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_context_keeps_both_sources_and_stable_blocks() {
        let old = (0..30).map(|n| format!("line {n}\r\n")).collect::<String>();
        let new = old
            .replace("line 10", "new 10")
            .replace("line 14", "new 14");
        let doc = Document::new(&old, &new);
        assert_eq!(doc.blocks.len(), 2);
        let folded = doc.rows(false, &Default::default());
        assert!(folded.iter().any(|r| matches!(r, DisplayRow::Fold { .. })));
        let all = doc.rows(false, &(0..doc.equals.len()).collect());
        assert_eq!(
            all.iter()
                .filter(|r| matches!(r, DisplayRow::Text { .. }))
                .count(),
            30
        );
        assert_eq!(doc.sources[0].text.as_ref(), old);
        assert_eq!(doc.sources[1].text.as_ref(), new);
    }

    #[test]
    fn inserted_padding_never_becomes_copyable_source() {
        let doc = Document::new("a\nb\n", "a\nx\ny\nb\n");
        assert_eq!(doc.blocks[0].old, 1..1);
        assert_eq!(doc.blocks[0].new, 1..3);
        let rows = doc.rows(false, &Default::default());
        assert!(rows.iter().any(|r| matches!(
            r,
            DisplayRow::Text {
                old: None,
                new: Some(1),
                ..
            }
        )));
        assert_eq!(&doc.sources[1].text[doc.sources[1].lines[1].clone()], "x");
    }
}

#[cfg(test)]
mod detail_tests {
    use super::*;
    #[test]
    fn original_newlines_and_empty_cr_lines_are_copyable() {
        let doc = Document::new("a\r\r\nb", "a\r\r\nc\n");
        assert_eq!(doc.sources[0].line(0), "a");
        assert_eq!(doc.sources[0].line(1), "");
        assert_eq!(doc.sources[0].line(2), "b");
        assert!(
            doc.rows(false, &Default::default())
                .contains(&DisplayRow::NoNewline { side: 0 })
        );
        assert_eq!(doc.sources[0].text.as_ref(), "a\r\r\nb");
    }
    #[test]
    fn refinement_spans_are_utf8_ranges_and_large_lines_remain_bounded() {
        let spans = intraline("цвет красный", "цвет синий");
        assert!(!spans[0].is_empty() && !spans[1].is_empty());
        for (side, text) in ["цвет красный", "цвет синий"].iter().enumerate() {
            for span in &spans[side] {
                assert!(text.get(span.clone()).is_some());
            }
        }
        assert_eq!(
            intraline(&"я".repeat(10000), "x"),
            [Vec::<Range<usize>>::new(), Vec::new()]
        );
    }
}

#[cfg(test)]
mod line_ending_regressions {
    use super::*;
    #[test]
    fn cr_lf_crlf_and_consecutive_terminators_share_source_and_diff_indices() {
        for text in ["a\rb\r", "a\nb\n", "a\r\nb\r\n", "a\r\r\nb", "я\rб\nв\r\n"] {
            let doc = Document::new(text, &text.replace('b', "c").replace('б', "г"));
            let all = doc.rows(false, &(0..doc.equals.len()).collect());
            for row in all {
                if let DisplayRow::Text { old, new, .. } = row {
                    assert!(
                        old.is_none_or(|i| i < doc.sources[0].lines.len()),
                        "old {old:?} in {text:?}"
                    );
                    assert!(
                        new.is_none_or(|i| i < doc.sources[1].lines.len()),
                        "new {new:?} in {text:?}"
                    );
                }
            }
            assert_eq!(doc.sources[0].text.as_ref(), text);
        }
    }
}

#[cfg(test)]
mod source_ending_values {
    use super::*;
    #[test]
    fn each_line_ending_retains_exact_bytes_with_the_same_content_rows() {
        for ending in ["\r", "\n", "\r\n"] {
            let text = format!("one{ending}two{ending}");
            let doc = Document::new(&text, "one\ntwo\n");
            assert_eq!(doc.sources[0].lines.len(), 2);
            assert_eq!(doc.sources[0].line(0), "one");
            assert_eq!(doc.sources[0].line(1), "two");
            assert_eq!(doc.sources[0].lines[1].start, 3 + ending.len());
            assert_eq!(doc.sources[0].text.as_ref(), text);
            assert!(
                doc.blocks.is_empty(),
                "line ending style must not invent content changes"
            );
            assert!(
                !doc.rows(false, &Default::default())
                    .contains(&DisplayRow::NoNewline { side: 0 })
            );
        }
    }
}
