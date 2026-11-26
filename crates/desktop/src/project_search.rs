//! Full document snapshots for project search. Only FileTab applies edits.
use crate::{
    document_search,
    highlight::Text,
    search::{self, Hit, Query, Refused, Summary},
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug)]
pub struct Buffer {
    pub text: String,
    pub clean: String,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub path: PathBuf,
    pub canonical: PathBuf,
    pub text: String,
    pub disk: Vec<u8>,
    pub buffer: bool,
    pub match_count: usize,
    pub refusal: Option<String>,
}
impl Snapshot {
    pub fn read(root: &Path, path: &Path, buffer: Option<&Buffer>) -> Result<Self, String> {
        let full = root.join(path);
        let canonical = full.canonicalize().map_err(|error| error.to_string())?;
        let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
        if !canonical.starts_with(&canonical_root) {
            return Err("File resolves outside the project".into());
        }
        let metadata = std::fs::metadata(&canonical).map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() > crate::file_tab::OPEN_LIMIT {
            return Err("Not an editable text file or file too large".into());
        }
        let disk = std::fs::read(&canonical).map_err(|error| error.to_string())?;
        let Text::Source { text, line_ending } = Text::of(&disk) else {
            return Err("Unsupported encoding or binary file".into());
        };
        let refusal = if metadata.permissions().readonly() {
            Some("Read-only file".into())
        } else if line_ending.apply(&text).as_bytes() != disk {
            Some("Mixed line endings cannot safely roundtrip".into())
        } else if buffer.is_some_and(|buffer| buffer.clean != text) {
            Some("Disk changed underneath the open buffer".into())
        } else {
            None
        };
        Ok(Self {
            path: path.into(),
            canonical,
            text: buffer.map_or(text, |buffer| buffer.text.clone()),
            disk,
            buffer: buffer.is_some(),
            match_count: 0,
            refusal,
        })
    }
    /// Background validation immediately before the UI applies a buffer transaction.
    pub fn validate(&self, root: &Path) -> Result<(), String> {
        if let Some(refusal) = &self.refusal {
            return Err(refusal.clone());
        }
        let full = root.join(&self.path);
        let canonical = full.canonicalize().map_err(|error| error.to_string())?;
        if canonical != self.canonical
            || !canonical.starts_with(root.canonicalize().map_err(|error| error.to_string())?)
        {
            return Err("File identity or symlink target changed".into());
        }
        if std::fs::metadata(&canonical)
            .map_err(|error| error.to_string())?
            .permissions()
            .readonly()
        {
            return Err("Read-only file".into());
        }
        if std::fs::read(&canonical).map_err(|error| error.to_string())? != self.disk {
            return Err("File changed since this search; search again".into());
        }
        Ok(())
    }
}

pub fn scan(
    root: &Path,
    query: &Query,
    buffers: &HashMap<PathBuf, Buffer>,
    stop: &AtomicBool,
    mut give: impl FnMut(Snapshot, Vec<Hit>),
) -> Result<Summary, Refused> {
    let mut summary = Summary::default();
    document_search::matches("", query, "", false).map_err(Refused::Pattern)?;
    if query.is_empty() {
        return Ok(summary);
    }
    let from = query
        .scope
        .as_ref()
        .map_or_else(|| root.to_path_buf(), |scope| root.join(scope));
    let mut walk = search::walker(&from);
    if let Some(mask) = search::mask(query)? {
        walk.overrides(mask);
    }
    let mut overlay_by_identity = HashMap::new();
    let mut conflicting = HashSet::new();
    for (path, buffer) in buffers {
        if let Ok(canonical) = root.join(path).canonicalize()
            && let Some((_, previous)) =
                overlay_by_identity.insert(canonical.clone(), (path, buffer))
            && previous.text != buffer.text
        {
            conflicting.insert(canonical);
        }
    }
    let mut seen = HashSet::new();
    let mut retained_bytes = 0usize;
    let mut occurrences = 0usize;
    for entry in walk.build().flatten() {
        if stop.load(Ordering::Relaxed) {
            summary.stopped = true;
            break;
        }
        if !entry
            .file_type()
            .is_some_and(|kind| kind.is_file() || kind.is_symlink())
        {
            continue;
        }
        let Ok(path) = entry.path().strip_prefix(root) else {
            continue;
        };
        let Ok(mut snapshot) = Snapshot::read(root, path, buffers.get(path)) else {
            continue;
        };
        if let Some((owner, buffer)) = overlay_by_identity.get(&snapshot.canonical) {
            let Ok(overlay) = Snapshot::read(root, owner, Some(buffer)) else {
                continue;
            };
            snapshot = overlay;
        }
        if conflicting.contains(&snapshot.canonical) {
            snapshot.refusal = Some("Open aliases have different unsaved buffers".into());
        }
        if !seen.insert(snapshot.canonical.clone()) {
            continue;
        }
        let Some(matches) =
            document_search::matches_cancellable(&snapshot.text, query, "", false, || {
                stop.load(Ordering::Relaxed)
            })
            .map_err(Refused::Pattern)?
        else {
            summary.stopped = true;
            break;
        };
        if matches.is_empty() {
            continue;
        }
        snapshot.match_count = matches.len();
        occurrences += matches.len();
        if occurrences > 100_000 {
            summary.capped = true;
            break;
        }
        retained_bytes += snapshot.text.len() + snapshot.disk.len();
        if retained_bytes > 64 * 1024 * 1024 {
            summary.capped = true;
            break;
        }
        let Some(hits) = document_hits(&snapshot, matches, &mut summary, || {
            stop.load(Ordering::Relaxed)
        }) else {
            summary.stopped = true;
            break;
        };
        summary.files += 1;
        summary.hits += hits.len();
        give(snapshot, hits);
        if summary.hits >= search::TOTAL {
            summary.capped = true;
            break;
        }
    }
    Ok(summary)
}

fn document_hits(
    snapshot: &Snapshot,
    matches: Vec<document_search::Match>,
    summary: &mut Summary,
    mut cancelled: impl FnMut() -> bool,
) -> Option<Vec<Hit>> {
    let mut hits: Vec<Hit> = Vec::new();
    let mut cursor = 0;
    let mut start = 0;
    let mut line = 1u32;
    let mut end = snapshot.text.find('\n').unwrap_or(snapshot.text.len());
    for found in matches {
        if cancelled() {
            return None;
        }
        // Every prefix byte is visited once, rather than once per match.
        for (at, byte) in snapshot.text.as_bytes()[cursor..found.range.start]
            .iter()
            .enumerate()
        {
            if at % 4096 == 0 && cancelled() {
                return None;
            }
            if *byte == b'\n' {
                line += 1;
                start = cursor + at + 1;
            }
        }
        cursor = found.range.start;
        if start > end {
            end = snapshot.text[start..]
                .find('\n')
                .map_or(snapshot.text.len(), |at| start + at);
        }
        if hits.last().is_some_and(|hit| hit.line == line) {
            if let Some(hit) = hits.last_mut() {
                hit.ranges
                    .push(found.range.start - start..found.range.end.min(end) - start);
            }
            continue;
        }
        if hits.len() >= search::PER_FILE || summary.hits + hits.len() >= search::TOTAL {
            summary.capped = true;
            break;
        }
        // Keep full line text in the model; previews and edits never use snippets.
        hits.push(Hit {
            path: snapshot.path.clone(),
            line,
            text: snapshot.text[start..end].into(),
            ranges: std::iter::once(found.range.start - start..found.range.end.min(end) - start)
                .collect(),
            cut: false,
            column: snapshot.text[start..found.range.start].chars().count() as u32,
        });
    }
    for hit in &mut hits {
        if cancelled() {
            return None;
        }
        let (text, ranges, cut) = search::cut_around(&hit.text, std::mem::take(&mut hit.ranges));
        hit.text = text;
        hit.ranges = ranges;
        hit.cut = cut;
    }
    Some(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hits_for(
        text: &str,
        pattern: &str,
        mut cancelled: impl FnMut() -> bool,
    ) -> Option<Vec<Hit>> {
        let snapshot = Snapshot {
            path: "a.txt".into(),
            canonical: "/fixture/a.txt".into(),
            text: text.into(),
            disk: Vec::new(),
            buffer: false,
            match_count: 0,
            refusal: None,
        };
        let edits = document_search::matches(
            text,
            &Query {
                text: pattern.into(),
                ..Default::default()
            },
            "",
            false,
        )
        .unwrap();
        document_hits(&snapshot, edits, &mut Summary::default(), &mut cancelled)
    }
    #[test]
    fn document_locations_follow_multiline_and_unicode_with_dense_rows() {
        let hits = hits_for("я cat cat\n🦀 cat\ncat\nend", "cat", || false).unwrap();
        assert_eq!(
            hits.iter().map(|h| (h.line, h.column)).collect::<Vec<_>>(),
            vec![(1, 2), (2, 2), (3, 0)]
        );
        assert_eq!(hits[0].ranges, vec![3..6, 7..10]);
        let hits = hits_for("я cat\ncat 🦀", "cat\ncat", || false).unwrap();
        assert_eq!((hits[0].line, hits[0].column), (1, 2));
        assert_eq!(hits[0].ranges, vec![3..6]);
        let prefix = "я".repeat(100_000);
        let text = prefix + &"cat".repeat(10_000);
        let mut checks = 0;
        let hits = hits_for(&text, "cat", || {
            checks += 1;
            false
        })
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].column, 100_000);
        assert!(hits[0].cut);
        // Cancellation checkpoints grow with bytes + matches, never prefixes * matches.
        assert!(checks < 21_000, "{checks}");
    }
    #[test]
    fn document_hit_processing_cancels_during_a_long_prefix() {
        let mut checks = 0;
        let text = "x".repeat(1024 * 1024) + &"cat".repeat(10_000);
        assert!(
            hits_for(&text, "cat", || {
                checks += 1;
                checks == 5
            })
            .is_none()
        );
        assert_eq!(checks, 5);
    }

    #[test]
    fn snapshots_preserve_dirty_text_and_refuse_stale_disk_and_mixed_newlines() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.txt"), "a\r\na").unwrap();
        let buffer = Buffer {
            clean: "a\na".into(),
            text: "unsaved a\na".into(),
        };
        let snapshot = Snapshot::read(root.path(), Path::new("a.txt"), Some(&buffer)).unwrap();
        assert_eq!(snapshot.text, "unsaved a\na");
        snapshot.validate(root.path()).unwrap();
        std::fs::write(root.path().join("a.txt"), "external").unwrap();
        assert!(snapshot.validate(root.path()).is_err());
        std::fs::write(root.path().join("a.txt"), "a\r\nb\n").unwrap();
        assert!(
            Snapshot::read(root.path(), Path::new("a.txt"), None)
                .unwrap()
                .validate(root.path())
                .is_err()
        );
    }
    #[test]
    fn links_outside_root_and_changed_targets_are_refused() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("x"), "text").unwrap();
        symlink(outside.path().join("x"), root.path().join("link")).unwrap();
        assert!(Snapshot::read(root.path(), Path::new("link"), None).is_err());
        std::fs::write(root.path().join("a"), "a").unwrap();
        std::fs::write(root.path().join("b"), "b").unwrap();
        std::fs::remove_file(root.path().join("link")).unwrap();
        symlink("a", root.path().join("link")).unwrap();
        let snapshot = Snapshot::read(root.path(), Path::new("link"), None).unwrap();
        std::fs::remove_file(root.path().join("link")).unwrap();
        symlink("b", root.path().join("link")).unwrap();
        assert!(snapshot.validate(root.path()).is_err());
    }
}
