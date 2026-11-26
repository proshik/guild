//! Runtime-only reuse of authenticated bytes, qualified by a fresh full listing.
use super::Release;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const MAX_ENTRIES: usize = 1000;
const MAX_BYTES: usize = 8 * 1024 * 1024;
// Longer than the window's 30-minute check cadence, never extended by a hit.
const MAX_AGE: Duration = Duration::from_secs(60 * 60);

struct Entry {
    release: Release,
    bytes: Vec<u8>,
    signature: Vec<u8>,
    inserted: Instant,
}
impl Entry {
    fn size(&self) -> usize {
        // Include the untrusted listing's strings in the retained byte budget.
        self.bytes.len()
            + self.signature.len()
            + self.release.tag_name.len()
            + self.release.updated_at.as_ref().map_or(0, String::len)
            + self
                .release
                .assets
                .iter()
                .map(|asset| {
                    std::mem::size_of_val(asset)
                        + asset.name.len()
                        + asset.updated_at.as_ref().map_or(0, String::len)
                        + asset.digest.as_ref().map_or(0, String::len)
                })
                .sum::<usize>()
    }
}
#[derive(Default)]
pub(super) struct Metadata {
    entries: VecDeque<Entry>,
}
impl Metadata {
    pub(super) fn get(&mut self, release: &Release) -> Option<(Vec<u8>, Vec<u8>)> {
        self.entries
            .retain(|entry| entry.inserted.elapsed() < MAX_AGE);
        self.entries
            .iter()
            .find(|entry| &entry.release == release)
            .map(|entry| (entry.bytes.clone(), entry.signature.clone()))
    }
    // Called only after signature, repository, tag, channel and archive checks succeed.
    pub(super) fn insert(&mut self, release: &Release, bytes: &[u8], signature: &[u8]) {
        let matches = |name: &str, bytes: &[u8]| {
            release
                .assets
                .iter()
                .find(|asset| asset.name == name)
                .and_then(|asset| asset.digest.as_deref())
                .is_some_and(|digest| digest == format!("sha256:{:x}", Sha256::digest(bytes)))
        };
        if !matches("guild-update.json", bytes) || !matches("guild-update.json.sig", signature) {
            return;
        }
        // A hit must not extend the original retention deadline.
        if self.entries.iter().any(|entry| &entry.release == release) {
            return;
        }
        let entry = Entry {
            release: release.clone(),
            bytes: bytes.to_vec(),
            signature: signature.to_vec(),
            inserted: Instant::now(),
        };
        let size = entry.size();
        if size > MAX_BYTES {
            return;
        }
        let mut retained: usize = self.entries.iter().map(Entry::size).sum();
        while self.entries.len() >= MAX_ENTRIES || retained + size > MAX_BYTES {
            if let Some(old) = self.entries.pop_front() {
                retained -= old.size();
            }
        }
        self.entries.push_back(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(id: u64) -> Release {
        serde_json::from_value(serde_json::json!({
            "id":id,"tag_name":"nightly-fixture","prerelease":true,"draft":false,
            "assets":[
                {"id":10,"name":"guild-update.json","size":1,"digest":format!("sha256:{:x}",Sha256::digest(b"x"))},
                {"id":11,"name":"guild-update.json.sig","size":1,"digest":format!("sha256:{:x}",Sha256::digest(b"s"))}
            ]
        })).unwrap()
    }
    #[test]
    fn hits_do_not_extend_expiry_and_expired_entries_are_removed() {
        let mut cache = Metadata::default();
        let release = release(1);
        cache.insert(&release, b"x", b"s");
        let inserted = Instant::now() - MAX_AGE + Duration::from_secs(5);
        cache.entries[0].inserted = inserted;
        assert!(cache.get(&release).is_some());
        cache.insert(&release, b"x", b"s");
        assert_eq!(cache.entries[0].inserted, inserted);
        cache.entries[0].inserted = Instant::now() - MAX_AGE;
        assert!(cache.get(&release).is_none());
        assert!(cache.entries.is_empty());
    }
    #[test]
    fn entry_and_byte_limits_evict_oldest_and_oversized_entries_are_not_kept() {
        let mut cache = Metadata::default();
        for id in 1..=MAX_ENTRIES as u64 + 1 {
            cache.insert(&release(id), b"x", b"s");
        }
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        assert!(cache.get(&release(1)).is_none());
        assert!(cache.get(&release(2)).is_some());
        cache.entries.clear();
        let mut large = release(1);
        large.tag_name = "x".repeat(MAX_BYTES / 2);
        cache.insert(&large, b"x", b"s");
        large.id = 2;
        cache.insert(&large, b"x", b"s");
        assert_eq!(cache.entries.len(), 1);
        large.id = 3;
        large.tag_name = "x".repeat(MAX_BYTES);
        cache.insert(&large, b"x", b"s");
        assert_eq!(cache.entries.len(), 1);
        assert!(cache.entries.iter().map(Entry::size).sum::<usize>() <= MAX_BYTES);
    }
    #[test]
    fn missing_or_mismatched_digests_disable_reuse() {
        for digest in [None, Some("sha256:replaced".into())] {
            let mut cache = Metadata::default();
            let mut release = release(1);
            release.assets[0].digest = digest;
            cache.insert(&release, b"x", b"s");
            assert!(cache.get(&release).is_none());
        }
    }
}
