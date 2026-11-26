//! Private SDK environment resources, with durable value-free ownership receipts.
use super::*;
use crate::workflow::{contract, json};
use std::{
    collections::BTreeMap,
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
};

/// Exact winning environment source/key. No provider value is serialized here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentReference {
    /// SDK executor receiving this override.
    pub executor: String,
    /// Original environment key, distinct for each executor.
    pub key: String,
    /// Qualified source file and exact bytes identity.
    pub source: Artifact,
    /// JSON pointer into that original winning policy or selected profile policy.
    pub pointer: String,
}
fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
pub(super) fn references(
    input: &serde_json::Value,
    user: &Path,
    caller: &Path,
) -> Result<Vec<EnvironmentReference>, String> {
    let mut result = Vec::new();
    let Some(executors) = input["policy"]["values"]["executors"].as_object() else {
        return Ok(result);
    };
    for (executor, settings) in executors {
        let Some(env) = settings.get("env") else {
            continue;
        };
        let env = env
            .as_object()
            .ok_or("revust.environment: expected environment map")?;
        for (key, redacted) in env {
            if !matches!(executor.as_str(), "claude" | "codex" | "opencode")
                || key.is_empty()
                || key.contains(['\0', '='])
                || !redacted.is_string()
            {
                return Err("revust.environment: invalid executor/key/value shape".into());
            }
            let pointer = format!("/executors/{}/env/{}", escape(executor), escape(key));
            let origin = &input["policy"]["origins"][&pointer];
            if origin["kind"] != "extension" {
                return Err("revust.environment: winning extension provenance missing".into());
            }
            let root = match origin["layer"].as_str() {
                Some("user") => user.to_path_buf(),
                Some("project") => caller.join(".revust"),
                _ => return Err("revust.environment: unsupported winning layer provenance".into()),
            }
            .canonicalize()
            .map_err(|_| "revust.environment: winning layer unavailable")?;
            let declared = PathBuf::from(
                origin["source"]
                    .as_str()
                    .ok_or("revust.environment: winning source missing")?,
            );
            let path = contract::contained(&root, Path::new("extensions.json"))?;
            if !declared.is_absolute()
                || declared
                    .canonicalize()
                    .map_err(|_| "revust.environment: declared source unavailable")?
                    != path
            {
                return Err("revust.environment: winning source escaped qualified layer".into());
            }
            let pointer = if let Some(profile) = origin["profile"].as_str() {
                if input["selected_profile"]["name"] != profile {
                    return Err("revust.environment: winning profile differs".into());
                }
                format!("/profiles/{}/policy{}", escape(profile), pointer)
            } else {
                format!("/policy{pointer}")
            };
            let bytes = contract::bounded_read(&path)
                .map_err(|_| "revust.environment: winning source unreadable")?;
            let raw = json::read(&bytes)
                .map_err(|_| "revust.environment: winning source JSON invalid")?;
            let value = raw
                .pointer(&pointer)
                .and_then(serde_json::Value::as_str)
                .ok_or("revust.environment: winning key is not a string")?;
            if value.contains('\0') {
                return Err("revust.environment: NUL environment value refused".into());
            }
            result.push(EnvironmentReference {
                executor: executor.clone(),
                key: key.clone(),
                source: Artifact {
                    path,
                    sha256: digest(&bytes),
                },
                pointer,
            });
        }
    }
    Ok(result)
}

/// Value-free ownership pointer. Keep this receipt across crashes; a saved PID
/// or observer exit cannot authorize cleanup while the native job may be live.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevustEnvironmentLease {
    /// Fresh private directory, outside durable review/run archives.
    pub directory: PathBuf,
    /// Fresh ownership nonce matching the private marker.
    pub owner: uuid::Uuid,
    /// Frozen configuration whose environment references were resolved.
    pub configuration: String,
    /// Exact prepared round owner, without any environment value.
    pub round: String,
    /// Private payload identity, without retaining the payload itself.
    pub sha256: String,
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "revust.environment: private resource creation failed")?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(|_| "revust.environment: private resource synchronization failed".into())
}
impl RevustEnvironmentLease {
    /// Resolve only the frozen winning file/key references and materialize a
    /// child-only SDK user layer. Diagnostics never contain file contents.
    pub fn create(snapshot: &RevustConfigSnapshot, round: &str) -> Result<Option<Self>, String> {
        snapshot.verify()?;
        if snapshot.environment.is_empty() {
            return Ok(None);
        }
        let mut executors = BTreeMap::<String, BTreeMap<String, String>>::new();
        for reference in &snapshot.environment {
            let bytes = contract::bounded_read(&reference.source.path).map_err(
                |_| "revust.environment: original referenced source unavailable; no launch",
            )?;
            if digest(&bytes) != reference.source.sha256 {
                return Err("revust.environment: original referenced source changed; explicit reconciliation required".into());
            }
            let raw = json::read(&bytes)
                .map_err(|_| "revust.environment: referenced source JSON invalid")?;
            let value = raw
                .pointer(&reference.pointer)
                .and_then(serde_json::Value::as_str)
                .filter(|v| !v.contains('\0'))
                .ok_or("revust.environment: referenced value unavailable")?;
            if executors
                .entry(reference.executor.clone())
                .or_default()
                .insert(reference.key.clone(), value.to_owned())
                .is_some()
            {
                return Err("revust.environment: duplicate reference".into());
            }
        }
        let executors: serde_json::Map<String, serde_json::Value> = executors
            .into_iter()
            .map(|(k, env)| (k, serde_json::json!({"env":env})))
            .collect();
        let payload =
            serde_json::to_vec(&serde_json::json!({"version":1,"policy":{"executors":executors}}))
                .map_err(|_| "revust.environment: private serialization failed")?;
        let owner = uuid::Uuid::new_v4();
        let directory = std::env::temp_dir()
            .canonicalize()
            .map_err(|_| "revust.environment: private temporary root unavailable")?
            .join(format!("guild-workflow-env-{owner}"));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .map_err(|_| "revust.environment: fresh private directory unavailable")?;
        let lease = Self {
            directory,
            owner,
            configuration: snapshot.sha256.clone(),
            round: round.into(),
            sha256: digest(&payload),
        };
        let result: Result<(), String> = (|| {
            private_write(
                &lease.directory.join("owner.json"),
                &serde_json::to_vec(&lease).map_err(|e| e.to_string())?,
            )?;
            private_write(&lease.directory.join("extensions.json"), &payload)?;
            std::fs::File::open(&lease.directory)
                .and_then(|f| f.sync_all())
                .map_err(|_| "revust.environment: private directory synchronization failed")?;
            lease.verify()?;
            Ok(())
        })();
        if let Err(error) = result {
            return match lease.release() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!(
                    "{error}; private resource retained at {} for owner {}: {cleanup}",
                    lease.directory.display(),
                    lease.owner
                )),
            };
        }
        Ok(Some(lease))
    }
    fn ownership(&self) -> Result<bool, String> {
        if self.directory.file_name()
            != Some(std::ffi::OsStr::new(&format!(
                "guild-workflow-env-{}",
                self.owner
            )))
            || self.directory.parent()
                != Some(
                    std::env::temp_dir()
                        .canonicalize()
                        .map_err(|_| "revust.environment: temporary root unavailable")?
                        .as_path(),
                )
        {
            return Err("revust.environment: private ownership path differs".into());
        }
        let metadata = match std::fs::symlink_metadata(&self.directory) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err("revust.environment: private owner unavailable".into()),
        };
        if !metadata.is_dir()
            || metadata.permissions().mode() & 0o777 != 0o700
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(
                "revust.environment: private directory ownership/permissions differ".into(),
            );
        }
        let marker = contract::bounded_read(&self.directory.join("owner.json"))
            .map_err(|_| "revust.environment: private owner marker unavailable")?;
        if serde_json::from_slice::<Self>(&marker)
            .map_err(|_| "revust.environment: invalid private owner marker")?
            != *self
        {
            return Err("revust.environment: private owner marker differs".into());
        }
        for entry in std::fs::read_dir(&self.directory)
            .map_err(|_| "revust.environment: private resources unavailable")?
        {
            let entry = entry.map_err(|_| "revust.environment: private resource unavailable")?;
            let name = entry.file_name();
            let m = std::fs::symlink_metadata(entry.path())
                .map_err(|_| "revust.environment: private resource metadata unavailable")?;
            if (name != "owner.json" && name != "extensions.json")
                || !m.is_file()
                || m.permissions().mode() & 0o777 != 0o600
                || m.uid() != metadata.uid()
            {
                return Err("revust.environment: private resource set/permissions differ; retained for owner reconciliation".into());
            }
        }
        Ok(true)
    }
    /// Read-only recovery observation. A retained layer is still owned; an
    /// absent layer is released only after its exact owner/path is qualified.
    /// Unknown/corrupt ownership is an error, never permission to recreate it.
    pub fn retained(&self) -> Result<bool, String> {
        if !self.ownership()? {
            return Ok(false);
        }
        self.verify()?;
        Ok(true)
    }
    /// Check private ownership before launcher admission, without logging values.
    pub fn verify(&self) -> Result<(), String> {
        if !self.ownership()? || !self.directory.join("extensions.json").is_file() {
            return Err(
                "revust.environment: private layer absent; no launch or automatic reconstruction"
                    .into(),
            );
        }
        let bytes = contract::bounded_read(&self.directory.join("extensions.json"))
            .map_err(|_| "revust.environment: private layer unreadable")?;
        if digest(&bytes) != self.sha256 {
            return Err("revust.environment: private bytes changed; no launch".into());
        }
        Ok(())
    }
    /// The coordinator calls this only after proven actual job termination, or
    /// preparation failure before dispatch. Uncertain jobs retain their lease.
    /// Already released owned paths are idempotent; no directory walk cleanup.
    pub fn release(&self) -> Result<(), String> {
        if !self.ownership()? {
            return Ok(());
        }
        let path = self.directory.join("extensions.json");
        if path
            .try_exists()
            .map_err(|_| "revust.environment: private resource state unknown")?
        {
            std::fs::remove_file(path)
                .map_err(|_| "revust.environment: private resource release failed")?;
        }
        std::fs::remove_file(self.directory.join("owner.json"))
            .map_err(|_| "revust.environment: private owner release failed")?;
        std::fs::remove_dir(&self.directory)
            .map_err(|_| "revust.environment: private directory release failed")?;
        std::fs::File::open(
            self.directory
                .parent()
                .ok_or("revust.environment: private parent unavailable")?,
        )
        .and_then(|f| f.sync_all())
        .map_err(|_| "revust.environment: private release directory synchronization failed")?;
        Ok(())
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn retained_layer_requires_exact_owner_and_corrupt_existing_layer_is_not_absence() {
        let owned = tempfile::tempdir().unwrap();
        let caller = owned.path().join("caller");
        let user = owned.path().join("user");
        let defaults = owned.path().join("defaults");
        for path in [&caller, &user, &defaults] {
            std::fs::create_dir(path).unwrap();
        }
        let source = user.join("extensions.json");
        std::fs::write(&source,serde_json::json!({"version":1,"policy":{"executors":{"codex":{"env":{"FIXTURE_ENV":"controlled-private-layer"}}}}}).to_string()).unwrap();
        let effective = serde_json::json!({"reference_revision":"revust-contract-v1","legacy":{"paths":{"config_dir":user},"knobs":[{"name":"profile","value":"fixture"}]},"selected_profile":{"name":"fixture"},"prompt_files":[],"policy":{"values":{"budget":{"max_calls":null},"executors":{"codex":{"env":{"FIXTURE_ENV":"<redacted>"}}}},"origins":{"/executors/codex/env/FIXTURE_ENV":{"kind":"extension","layer":"user","source":source}}}});
        let snapshot = RevustConfigSnapshot::capture(
            &caller,
            &defaults,
            &serde_json::to_vec(&effective).unwrap(),
            &owned.path().join("control"),
            2,
        )
        .unwrap();
        // Production creates/qualifies the exact private path. This scope owns
        // only that fresh lease and restores its deliberate corruption on panic.
        struct Layer {
            lease: RevustEnvironmentLease,
            marker: Vec<u8>,
            payload: Vec<u8>,
        }
        impl Drop for Layer {
            fn drop(&mut self) {
                if self.lease.directory.is_dir() {
                    let _ = std::fs::write(self.lease.directory.join("owner.json"), &self.marker);
                    let _ =
                        std::fs::write(self.lease.directory.join("extensions.json"), &self.payload);
                    self.lease
                        .release()
                        .expect("release the exact test-owned private layer");
                }
            }
        }
        let lease = RevustEnvironmentLease::create(&snapshot, "controlled-recovery")
            .unwrap()
            .unwrap();
        let layer = Layer {
            marker: std::fs::read(lease.directory.join("owner.json")).unwrap(),
            payload: std::fs::read(lease.directory.join("extensions.json")).unwrap(),
            lease,
        };
        let lease = &layer.lease;
        let marker = &layer.marker;
        assert_eq!(lease.retained(), Ok(true));
        std::fs::write(lease.directory.join("owner.json"), b"{}").unwrap();
        assert!(lease.retained().is_err());
        std::fs::write(lease.directory.join("owner.json"), marker).unwrap();
        std::fs::write(lease.directory.join("extensions.json"), b"corrupt").unwrap();
        assert!(lease.retained().is_err());
        std::fs::write(lease.directory.join("extensions.json"), &layer.payload).unwrap();
        lease.release().unwrap();
        assert_eq!(lease.retained(), Ok(false));
        let mut foreign = lease.clone();
        foreign.directory = owned.path().join("not-owned-by-this-lease");
        assert!(foreign.retained().is_err());
    }
}
