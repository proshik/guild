//! Bounded HEAD-to-buffer line markers, independent of rendering and Git IO.
use similar::{Algorithm, DiffTag, TextDiff};
use std::{
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

pub const BYTE_LIMIT: usize = 2 * 1024 * 1024;
pub const LINE_LIMIT: usize = 20_000;
pub const MARK_LIMIT: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Added,
    Modified,
    Deleted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Marker {
    /// Zero-based current LF row; Deleted denotes the boundary before it.
    /// At EOF this can equal the number of non-phantom current lines.
    pub row: usize,
    pub kind: Kind,
}

/// A contiguous non-equal operation, in normalized UTF-8 snapshot offsets.
#[derive(Clone, Debug)]
pub struct Block {
    pub old: Range<usize>,
    pub current: Range<usize>,
    /// Exact bytes in the original `new` argument, before CRLF normalization.
    pub current_raw: Range<usize>,
    pub rows: Range<usize>,
    pub old_rows: Range<usize>,
}
#[derive(Clone, Debug)]
pub struct Comparison {
    pub old: Arc<str>,
    pub current: Arc<str>,
    pub blocks: Vec<Block>,
    pub markers: Vec<Marker>,
    /// Parallel to markers; a replacement and its excess deletion share a block.
    pub marker_blocks: Vec<usize>,
}
pub fn between(old: &str, new: &str) -> Option<Vec<Marker>> {
    comparison(old, new).map(|comparison| comparison.markers)
}
/// None means unsupported/budget overflow, never a partial set. Bare CR remains
/// in its actual retained-editor LF row; CRLF is normalized like FileTab reads.
pub fn comparison(old: &str, new: &str) -> Option<Comparison> {
    if old.len() > BYTE_LIMIT || new.len() > BYTE_LIMIT {
        return None;
    }
    let raw_new = new;
    let old = old.replace("\r\n", "\n");
    let new = new.replace("\r\n", "\n");
    if old.bytes().filter(|b| *b == b'\n').count() >= LINE_LIMIT
        || new.bytes().filter(|b| *b == b'\n').count() >= LINE_LIMIT
    {
        return None;
    }
    let before: Vec<_> = old.split_inclusive('\n').collect();
    let after: Vec<_> = new.split_inclusive('\n').collect();
    let started = Instant::now();
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .timeout(Duration::from_millis(50))
        .diff_slices(&before, &after);
    if started.elapsed() >= Duration::from_millis(50) {
        return None;
    }
    let offsets = |lines: &[&str]| {
        let mut at = 0;
        let mut starts = Vec::with_capacity(lines.len() + 1);
        starts.push(at);
        for line in lines {
            at += line.len();
            starts.push(at);
        }
        starts
    };
    let old_offsets = offsets(&before);
    let current_offsets = offsets(&after);
    // CRLF changes byte counts, not LF row identities. Keep the original
    // boundaries for atomic edits into a Rope that may contain pasted CRLF.
    let raw_offsets: Vec<_> = std::iter::once(0)
        .chain(raw_new.split_inclusive('\n').scan(0, |at, line| {
            *at += line.len();
            Some(*at)
        }))
        .collect();
    let mut blocks = Vec::new();
    let mut marker_blocks = Vec::new();
    let mut marks = Vec::new();
    for op in diff.ops() {
        let old = op.old_range();
        let new = op.new_range();
        let paired = old.len().min(new.len());
        let count = match op.tag() {
            DiffTag::Equal => 0,
            DiffTag::Delete => 1,
            DiffTag::Insert => new.len(),
            DiffTag::Replace => new.len() + usize::from(old.len() > new.len()),
        };
        if marks.len() + count > MARK_LIMIT {
            return None;
        }
        if count > 0 {
            marker_blocks.extend(std::iter::repeat_n(blocks.len(), count));
            blocks.push(Block {
                old: old_offsets[old.start]..old_offsets[old.end],
                current: current_offsets[new.start]..current_offsets[new.end],
                current_raw: raw_offsets[new.start]..raw_offsets[new.end],
                rows: new.clone(),
                old_rows: old.clone(),
            });
        }
        match op.tag() {
            DiffTag::Equal => {}
            DiffTag::Delete => marks.push(Marker {
                row: new.start,
                kind: Kind::Deleted,
            }),
            DiffTag::Insert | DiffTag::Replace => {
                for row in new.clone() {
                    marks.push(Marker {
                        row,
                        kind: if op.tag() == DiffTag::Replace && row < new.start + paired {
                            Kind::Modified
                        } else {
                            Kind::Added
                        },
                    });
                }
                if old.len() > new.len() {
                    marks.push(Marker {
                        row: new.end,
                        kind: Kind::Deleted,
                    });
                }
            }
        }
    }
    Some(Comparison {
        old: old.into(),
        current: new.into(),
        blocks,
        markers: marks,
        marker_blocks,
    })
}
