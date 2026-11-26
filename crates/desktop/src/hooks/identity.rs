//! Durable evidence from a Codex hook, independent of socket delivery.

use guild_session::{HarnessKind, SessionId, SessionRecord};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const DIRECTORY_VAR: &str = "GUILD_IDENTITY_DIR";

pub fn directory() -> PathBuf {
    crate::paths::state_dir().join("conversation-identities")
}

/// Only one exact candidate is evidence. Never choose by folder, mtime or
/// event order; an in-TUI resume can leave several conversations for a launch.
pub fn recover(root: &Path, record: &SessionRecord) -> Option<String> {
    if record.runs.harness() != Some(HarnessKind::Codex)
        || record
            .harness_session
            .as_deref()
            .is_some_and(|id| !id.is_empty())
    {
        return None;
    }
    let mut found = None;
    for entry in std::fs::read_dir(root.join(record.id.to_string())).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        // Malformed committed evidence fails closed; temporary writes are ignored.
        let mut text = String::new();
        std::fs::File::open(&path)
            .ok()?
            .take(1025)
            .read_to_string(&mut text)
            .ok()?;
        if text.len() > 1024 {
            return None;
        }
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        let session: SessionId = value.get("guild_session")?.as_str()?.parse().ok()?;
        let conversation = value.get("codex_session")?.as_str()?;
        let _: SessionId = conversation.parse().ok()?;
        if session != record.id || path.file_stem()?.to_str()? != conversation {
            return None;
        }
        if found.as_deref().is_some_and(|known| known != conversation) {
            return None;
        }
        found = Some(conversation.to_owned());
    }
    found
}
