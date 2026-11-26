//! Legacy-safe preferences and explicit recovery from uncertain publication.
use super::{Channel, Preferences, Source, lock::FileLock};
use crate::{Normalise, Store};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;
const FILE: &str = "update-preferences.json";
const INTENT: &str = "update-preferences.pending.json";
const LIMIT: u64 = 128 * 1024;
/// A durable preference record and its recovery state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferenceState {
    /// Ready for update operations.
    Ready(Preferences),
    /// A save may have been published but was not durably acknowledged.
    Recovery {
        /// Identity required by an explicit recovery decision.
        operation_id: Uuid,
        /// Previously confirmed value.
        old: Preferences,
        /// Requested value.
        requested: Preferences,
        /// Value actually read back; None means publication is indeterminate.
        published: Option<Preferences>,
    },
}
impl PreferenceState {
    /// Actual visible selection; recovery never grants permission to install.
    pub fn displayed(&self) -> &Preferences {
        match self {
            Self::Ready(p) => p,
            Self::Recovery { old, published, .. } => published.as_ref().unwrap_or(old),
        }
    }
    /// Whether checking/installing with these settings is allowed.
    pub fn ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Document {
    schema: u32,
    operation_id: Uuid,
    source: String,
    channel: String,
    local_directory: Option<PathBuf>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            schema: 1,
            operation_id: Uuid::nil(),
            source: "remote".into(),
            channel: "nightly".into(),
            local_directory: None,
            extra: BTreeMap::new(),
        }
    }
}
impl Normalise for Document {}
impl Document {
    fn preferences(&self) -> Result<Preferences, String> {
        if self.schema != 1 {
            return Err("unsupported update preference schema; file preserved".into());
        }
        let source = match self.source.as_str() {
            "local" => Source::Local,
            "remote" => Source::Remote,
            _ => return Err("unknown update source; file preserved".into()),
        };
        let channel = match self.channel.as_str() {
            "nightly" => Channel::Nightly,
            "stable" => Channel::Stable,
            _ => return Err("unknown update channel; file preserved".into()),
        };
        if self
            .local_directory
            .as_ref()
            .is_some_and(|p| !p.is_absolute())
        {
            return Err("local update directory must be absolute".into());
        }
        Ok(Preferences {
            source,
            channel,
            local_directory: self.local_directory.clone(),
        })
    }
    fn changing(&self, p: &Preferences) -> Result<Self, String> {
        let mut next = self.clone();
        next.operation_id = Uuid::new_v4();
        next.source = p.source.name().into();
        next.channel = p.channel.name().into();
        next.local_directory = p.local_directory.clone();
        next.preferences()?;
        Ok(next)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Pending {
    schema: u32,
    old: Document,
    prior: Document,
    requested: Document,
    #[serde(default)]
    settled: bool,
}
impl Normalise for Pending {}
/// Updater choices in an isolated state directory, untouched by legacy Settings.
/// All methods perform IO and belong on an owned worker, never the window thread.
#[derive(Debug, Clone)]
pub struct PreferenceStore {
    state: PathBuf,
}
impl PreferenceStore {
    /// Construct without touching disk or reading any credentials.
    pub fn new(state: PathBuf) -> Self {
        Self { state }
    }
    fn lock(&self) -> Result<FileLock, String> {
        FileLock::acquire(
            &self.state,
            ".guild-update-preferences.lock",
            Duration::from_secs(5),
        )
        .map_err(|e| e.to_string())
    }
    fn document(&self) -> Result<Document, String> {
        match read::<Document>(&self.state.join(FILE))? {
            Some(d) => Ok(d),
            None => {
                Ok(read::<Document>(&self.state.join(format!("{FILE}.bak")))?.unwrap_or_default())
            }
        }
    }
    fn pending(&self) -> Result<Option<Pending>, String> {
        let p = match read::<Pending>(&self.state.join(INTENT))? {
            Some(p) => Some(p),
            None => read::<Pending>(&self.state.join(format!("{INTENT}.bak")))?,
        };
        if let Some(p) = &p {
            if p.schema != 1 {
                return Err("unknown update recovery schema; files preserved".into());
            }
            p.old.preferences()?;
            p.requested.preferences()?;
        }
        Ok(p)
    }
    fn recovery(&self, p: &Pending) -> Result<PreferenceState, String> {
        let published = match self.document() {
            Ok(d)
                if d.operation_id == p.old.operation_id
                    || d.operation_id == p.prior.operation_id
                    || d.operation_id == p.requested.operation_id =>
            {
                Some(d.preferences()?)
            }
            // Keep the intent and prevent updates rather than overwriting an unrelated writer.
            _ => None,
        };
        Ok(PreferenceState::Recovery {
            operation_id: p.requested.operation_id,
            old: p.old.preferences()?,
            requested: p.requested.preferences()?,
            published,
        })
    }
    /// Read without rewriting unknown data; an unsettled save requires a decision.
    pub fn load(&self) -> Result<PreferenceState, String> {
        if !self.state.exists() {
            return Ok(PreferenceState::Ready(Preferences::default()));
        }
        let _lock = self.lock()?;
        self.load_locked()
    }
    fn load_locked(&self) -> Result<PreferenceState, String> {
        if let Some(p) = self.pending()? {
            if p.settled {
                let actual = self.document()?;
                if actual.operation_id == p.requested.operation_id {
                    return Ok(PreferenceState::Ready(actual.preferences()?));
                }
            }
            return self.recovery(&p);
        }
        Ok(PreferenceState::Ready(self.document()?.preferences()?))
    }
    /// Save under an OS lock with a durable intent before modifying the canonical file.
    pub fn save(&self, value: &Preferences) -> Result<PreferenceState, String> {
        self.save_with(value, None, |store, doc| {
            store.save(doc).map_err(|e| e.to_string())
        })
    }
    /// Explicitly keep the requested selection or restore the previous confirmed one.
    pub fn resolve(
        &self,
        operation_id: Uuid,
        keep_requested: bool,
    ) -> Result<PreferenceState, String> {
        let pending = {
            let _lock = self.lock()?;
            self.pending()?.ok_or("no preference save needs recovery")?
        };
        if pending.settled || pending.requested.operation_id != operation_id {
            return Err("preference recovery changed; reload before deciding".into());
        }
        let chosen = if keep_requested {
            pending.requested.preferences()?
        } else {
            pending.old.preferences()?
        };
        self.save_with(&chosen, Some(operation_id), |store, doc| {
            store.save(doc).map_err(|e| e.to_string())
        })
    }
    fn save_with(
        &self,
        value: &Preferences,
        recovering: Option<Uuid>,
        write: impl FnOnce(&Store<Document>, &Document) -> Result<(), String>,
    ) -> Result<PreferenceState, String> {
        let _lock = self.lock()?;
        let recorded = self.pending()?;
        if let Some(p) = &recorded
            && p.settled
            && self.document()?.operation_id != p.requested.operation_id
        {
            return Err("settled preferences changed outside the updater; files preserved".into());
        }
        let pending = recorded.filter(|p| !p.settled);
        if pending.is_some() && recovering.is_none() {
            return Err("resolve the interrupted update preference save first".into());
        }
        if let Some(expected) = recovering
            && pending.as_ref().map(|p| p.requested.operation_id) != Some(expected)
        {
            return Err("preference recovery changed; reload before deciding".into());
        }
        let actual = self.document()?;
        if let Some(p) = &pending
            && actual.operation_id != p.old.operation_id
            && actual.operation_id != p.prior.operation_id
            && actual.operation_id != p.requested.operation_id
        {
            return Err(
                "preference record changed outside its pending operation; files preserved".into(),
            );
        }
        let old = if let Some(p) = &pending {
            p.old.clone()
        } else {
            actual.clone()
        };
        old.preferences()?;
        let requested = actual.changing(value)?;
        let mut intent = Pending {
            schema: 1,
            old,
            prior: actual,
            requested,
            settled: false,
        };
        if serde_json::to_vec(&intent)
            .map_err(|e| e.to_string())?
            .len() as u64
            > LIMIT
        {
            return Err("update preference recovery record would exceed size limit".into());
        }
        check_store_paths(&self.state.join(INTENT))?;
        Store::new(self.state.join(INTENT))
            .save(&intent)
            .map_err(|e| e.to_string())?;
        check_store_paths(&self.state.join(FILE))?;
        let store = Store::new(self.state.join(FILE));
        if write(&store, &intent.requested).is_err() {
            return self.recovery(&intent);
        }
        let readback = self.document()?;
        if readback.operation_id != intent.requested.operation_id {
            return self.recovery(&intent);
        }
        // Retire the intent by a settled record, never by deleting the only
        // crash witness before syncing that deletion. Canonical data is already
        // durable here. Read back settlement even if its own final sync failed:
        // a remaining pending record conservatively requires recovery next time.
        intent.settled = true;
        let _ = Store::new(self.state.join(INTENT)).save(&intent);
        self.load_locked()
    }
}
fn check_store_paths(path: &Path) -> Result<(), String> {
    for suffix in ["", ".tmp", ".bak"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        if let Ok(m) = std::fs::symlink_metadata(PathBuf::from(name))
            && (!m.is_file() || m.file_type().is_symlink())
        {
            return Err("update settings path is not a regular owned file".into());
        }
    }
    Ok(())
}
fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if !file
        .metadata()
        .map_err(|_| "could not inspect update preferences")?
        .is_file()
    {
        return Err("update preferences are not a regular file; preserved".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("update preferences exceed size limit; file preserved".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "unreadable update preferences; file preserved".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_directory_sync_after_publication_requires_recovery_even_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let store = PreferenceStore::new(root.path().into());
        let next = Preferences {
            source: Source::Local,
            channel: Channel::Stable,
            local_directory: Some(root.path().join("builds")),
        };
        let result = store
            .save_with(&next, None, |s, d| {
                s.save(d).unwrap();
                Err("injected final directory sync failure".into())
            })
            .unwrap();
        assert!(matches!(result,PreferenceState::Recovery {published:Some(ref p),..} if p==&next));
        let restarted = PreferenceStore::new(root.path().into());
        let PreferenceState::Recovery { operation_id, .. } = restarted.load().unwrap() else {
            panic!("pending save")
        };
        assert_eq!(
            restarted.resolve(operation_id, false).unwrap(),
            PreferenceState::Ready(Preferences::default())
        );
        assert_eq!(
            restarted.load().unwrap(),
            PreferenceState::Ready(Preferences::default())
        );
    }
    #[test]
    fn prepublication_failure_does_not_silently_choose_the_requested_source() {
        let root = tempfile::tempdir().unwrap();
        let store = PreferenceStore::new(root.path().into());
        let next = Preferences {
            source: Source::Local,
            ..Default::default()
        };
        let result = store
            .save_with(&next, None, |_, _| Err("injected before rename".into()))
            .unwrap();
        assert_eq!(result.displayed(), &Preferences::default());
        assert!(!result.ready());
        let PreferenceState::Recovery { operation_id, .. } = result else {
            panic!("pending save")
        };
        assert_eq!(
            store.resolve(operation_id, true).unwrap(),
            PreferenceState::Ready(next)
        );
    }
    #[test]
    fn interrupted_intent_rename_cannot_hide_an_unacknowledged_source_change() {
        let root = tempfile::tempdir().unwrap();
        let store = PreferenceStore::new(root.path().into());
        let next = Preferences {
            source: Source::Local,
            ..Default::default()
        };
        store
            .save_with(&next, None, |s, d| {
                s.save(d).unwrap();
                std::fs::rename(
                    root.path().join(INTENT),
                    root.path().join(format!("{INTENT}.bak")),
                )
                .unwrap();
                Err("interrupted intent replacement".into())
            })
            .unwrap();
        assert!(
            !PreferenceStore::new(root.path().into())
                .load()
                .unwrap()
                .ready(),
            "restart must read the remaining recovery backup"
        );
    }
}
