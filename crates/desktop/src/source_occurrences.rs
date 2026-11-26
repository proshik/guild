//! Bounded identifier matching shared by editors and immutable source viewers.
use std::ops::Range;
const MAX_OCCURRENCES: usize = 4096;

fn continues(ch: char) -> bool {
    unicode_ident::is_xid_continue(ch)
}

pub(crate) fn identifier(text: &str, caret: usize) -> Option<&str> {
    // The character on the caret's right must belong to the identifier:
    // whitespace, punctuation and EOF clear rather than borrowing the left word.
    let right = text.get(caret..)?;
    if !right.chars().next().is_some_and(continues) {
        return None;
    }
    let start = text[..caret]
        .char_indices()
        .rev()
        .find(|(_, ch)| !continues(*ch))
        .map_or(0, |(at, ch)| at + ch.len_utf8());
    let end = right
        .char_indices()
        .find(|(_, ch)| !continues(*ch))
        .map_or(text.len(), |(at, _)| caret + at);
    let word = &text[start..end];
    word.chars()
        .next()
        .filter(|ch| *ch == '_' || unicode_ident::is_xid_start(*ch))
        .map(|_| word)
}

pub(crate) fn mentions(text: &str, word: &str) -> Vec<Range<usize>> {
    if text.len() as u64 > crate::file_tab::HIGHLIGHT_LIMIT {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    for (start, _) in text.match_indices(word) {
        let end = start + word.len();
        let left = text[..start].chars().next_back();
        let right = text[end..].chars().next();
        if !left.is_some_and(continues) && !right.is_some_and(continues) {
            if ranges.len() == MAX_OCCURRENCES {
                // Never install a misleading partial result. Stop at the first
                // excess match, before allocating another range or decoration.
                return Vec::new();
            }
            ranges.push(start..end);
        }
    }
    ranges
}

/// Visible line-local ranges from a bounded sorted set of original UTF-8 ranges.
pub(crate) fn within(ranges: &[Range<usize>], line: &Range<usize>) -> Vec<Range<usize>> {
    let first = ranges.partition_point(|range| range.end <= line.start);
    ranges[first..]
        .iter()
        .take_while(|range| range.start < line.end)
        .map(|range| range.start.saturating_sub(line.start)..range.end.min(line.end) - line.start)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_matcher_rejects_large_sparse_and_dense_snapshots_without_partial_results() {
        let large = format!(
            "{} name name",
            " ".repeat(crate::file_tab::HIGHLIGHT_LIMIT as usize)
        );
        assert!(mentions(&large, "name").is_empty());
        assert!(mentions(&"a;".repeat(4097), "a").is_empty());
        assert_eq!(mentions(&"a;".repeat(4096), "a").len(), 4096);
        assert_eq!(mentions("name name", "name"), vec![0..4, 5..9]);
    }
}
