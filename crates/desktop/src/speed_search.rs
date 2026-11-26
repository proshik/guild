//! Transient matching over a host's current expanded rows. No filtering or I/O.
use std::ops::Range;

/// Literal and keyboard-position alternatives, with byte ranges in the original label.
pub fn ranges(label: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut folded = String::new();
    let mut offsets = Vec::new();
    for (start, c) in label.char_indices() {
        for lower in c.to_lowercase() {
            folded.push(lower);
            offsets.extend(std::iter::repeat_n(
                start..start + c.len_utf8(),
                lower.len_utf8(),
            ));
        }
    }
    let alternatives = [
        query.to_lowercase(),
        mapped(query, false),
        mapped(query, true),
    ];
    let mut found = Vec::new();
    for query in alternatives {
        for (at, _) in folded.char_indices() {
            if folded[at..].starts_with(&query) {
                found.push(offsets[at].start..offsets[at + query.len() - 1].end);
            }
        }
    }
    found.sort_by_key(|r| (r.start, r.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in found {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

fn mapped(query: &str, reverse: bool) -> String {
    // US/Russian keyboard positions, including differing Shift number-row
    // symbols. Both directions are separate alternatives: e.g. a quote can be
    // US Shift-apostrophe (RU Э) or Russian Shift-2 (US @).
    let en = "`qwertyuiop[]asdfghjkl;'zxcvbnm,./~{}:\"<>?@#$^&|";
    let ru = "ёйцукенгшщзхъфывапролджэячсмитьбю.ЁХЪЖЭБЮ,\"№;:?/";
    let (from, to) = if reverse { (ru, en) } else { (en, ru) };
    query
        .chars()
        .map(|c| {
            from.chars()
                .position(|k| k == c)
                // Shift punctuation must be mapped before folding case. Other
                // uppercase letters share the lowercase keyboard position.
                .or_else(|| {
                    from.chars()
                        .position(|k| Some(k) == c.to_lowercase().next())
                })
                .and_then(|i| to.chars().nth(i))
                .unwrap_or(c)
        })
        .collect::<String>()
        .to_lowercase()
}

#[derive(Clone, Debug)]
pub struct Candidate<K> {
    pub key: K,
    pub label: String,
    pub row: usize,
}
impl<K> Candidate<K> {
    pub fn new(key: K, label: impl Into<String>, row: usize) -> Self {
        Self {
            key,
            label: label.into(),
            row,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Search<K> {
    pub query: String,
    pub current: Option<K>,
}
impl<K: Clone + Eq> Search<K> {
    pub fn new(query: impl Into<String>, current: Option<K>) -> Self {
        Self {
            query: query.into(),
            current,
        }
    }
    /// Resolve against current candidates even if the host refreshed since the last input.
    pub fn matched<'a>(&self, rows: &'a [Candidate<K>]) -> Option<&'a Candidate<K>> {
        if self.query.is_empty() {
            return None;
        }
        let start = self
            .current
            .as_ref()
            .and_then(|key| rows.iter().position(|r| &r.key == key))
            .unwrap_or(0);
        rows[start..]
            .iter()
            .chain(&rows[..start])
            .find(|r| !ranges(&r.label, &self.query).is_empty())
    }
    pub fn reconcile(&mut self, rows: &[Candidate<K>]) -> Option<&K> {
        if self
            .current
            .as_ref()
            .is_some_and(|key| !rows.iter().any(|r| &r.key == key))
        {
            self.current = None;
        }
        let key = self.matched(rows)?.key.clone();
        self.current = Some(key);
        self.current.as_ref()
    }
    pub fn step(&mut self, rows: &[Candidate<K>], backwards: bool) -> Option<&K> {
        let matches: Vec<_> = rows
            .iter()
            .filter(|r| !self.query.is_empty() && !ranges(&r.label, &self.query).is_empty())
            .collect();
        if matches.is_empty() {
            return None;
        }
        let current = self
            .current
            .as_ref()
            .and_then(|key| matches.iter().position(|r| &r.key == key));
        let next = match current {
            Some(i) if backwards => (i + matches.len() - 1) % matches.len(),
            Some(i) => (i + 1) % matches.len(),
            None if backwards => matches.len() - 1,
            None => 0,
        };
        self.current = Some(matches[next].key.clone());
        self.current.as_ref()
    }
}
