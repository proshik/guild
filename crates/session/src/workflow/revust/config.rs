//! Frozen effective settings and exact winning prompt resources.
use super::*;
use crate::workflow::{contract, json};

/// Frozen winning review settings/resources. Original configuration is never
/// rewritten, and changed installed profiles cannot change these saved bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevustConfigSnapshot {
    /// Owned caller/control cwd; `.revust` below it supplies the only config layer.
    pub directory: PathBuf,
    /// Actual selected external profile, preserved rather than inferred from harness.
    pub profile: String,
    /// Original resolved roster/model/effort and source provenance.
    pub selected_profile: serde_json::Value,
    /// Effective policy with an enforceable allowance no larger than the reservation.
    pub policy: serde_json::Value,
    /// Digest of actual original read-only config inspection output.
    pub original_sha256: String,
    /// Saved exact resource/config bytes, relative to the owned control cwd.
    pub files: Vec<Artifact>,
    /// Aggregate identity of all saved metadata and resources.
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_budget: Option<OriginalBudget>,
    /// Qualified original environment references, never the referenced values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment: Vec<super::environment::EnvironmentReference>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalBudget {
    max_calls: Option<u64>,
}
impl RevustConfigSnapshot {
    /// Prepare a fresh run-owned config from actual effective inspection and
    /// embedded defaults exported by the qualified installed binary. This method
    /// starts no process. The host validates the saved result through that same
    /// binary before launcher admission.
    pub fn capture(
        caller: &Path,
        defaults: &Path,
        effective: &[u8],
        destination: &Path,
        allowance: u32,
    ) -> Result<Self, String> {
        use std::collections::BTreeMap;
        if allowance == 0 {
            return Err("revust.config: zero remaining allowance forbids launch".into());
        }
        let input = json::read(effective)?;
        if input["reference_revision"] != "revust-contract-v1" {
            return Err("revust.config: unsupported effective-config contract".into());
        }
        let caller = caller.canonicalize().map_err(|e| e.to_string())?;
        let defaults = defaults.canonicalize().map_err(|e| e.to_string())?;
        let user = PathBuf::from(
            input["legacy"]["paths"]["config_dir"]
                .as_str()
                .ok_or("revust.config: actual config root missing")?,
        );
        if !user.is_absolute() {
            return Err("revust.config: expected qualified absolute user root".into());
        }
        let profile = input["selected_profile"]["name"]
            .as_str()
            .filter(|p| {
                !p.is_empty()
                    && p.len() <= 128
                    && p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            })
            .ok_or("revust.config: selected profile identity missing")?
            .to_owned();
        let mut policy = input["policy"]["values"].clone();
        if !policy.is_object() {
            return Err("revust.config: effective policy unavailable".into());
        }
        let environment = super::environment::references(&input, &user, &caller)?;
        if let Some(executors) = policy["executors"].as_object_mut() {
            for executor in executors.values_mut() {
                if executor.get("env").is_some() {
                    executor["env"] = serde_json::json!({});
                }
            }
        }
        let original_budget = Some(OriginalBudget {
            max_calls: policy["budget"]["max_calls"].as_u64(),
        });
        let selected_cap = match &policy["budget"]["max_calls"] {
            serde_json::Value::Null => allowance as u64,
            value => value
                .as_u64()
                .filter(|v| *v > 0)
                .ok_or("revust.config: invalid existing max_calls")?
                .min(allowance as u64),
        };
        if !policy["budget"].is_object() {
            policy["budget"] = serde_json::json!({});
        }
        policy["budget"]["max_calls"] = serde_json::json!(selected_cap);
        let mut resources = BTreeMap::new();
        for item in input["prompt_files"]
            .as_array()
            .ok_or("revust.config: winning prompt manifest missing")?
        {
            let relative = PathBuf::from(
                item["path"]
                    .as_str()
                    .ok_or("revust.config: prompt path missing")?,
            );
            if !relative.starts_with("prompts") && !relative.starts_with("lenses") {
                return Err("revust.config: unexpected winning prompt namespace".into());
            }
            let source_root = match item["layer"].as_str() {
                Some("embedded") => defaults.clone(),
                Some("user") => user.canonicalize().map_err(|e| e.to_string())?,
                Some("project") => caller
                    .join(".revust")
                    .canonicalize()
                    .map_err(|e| e.to_string())?,
                _ => return Err("revust.config: unqualified prompt layer".into()),
            };
            let source = contract::contained(&source_root, &relative)?;
            let destination = PathBuf::from(".revust").join(&relative);
            if resources
                .insert(destination, contract::bounded_read(&source)?)
                .is_some()
            {
                return Err("revust.config: duplicate winning prompt resource".into());
            }
        }
        let mut extensions = serde_json::json!({"version":1,"policy":policy});
        if let Some(source) = input["selected_profile"]["source"].as_str() {
            let source = PathBuf::from(source)
                .canonicalize()
                .map_err(|e| e.to_string())?;
            let roots = [user.clone(), caller.join(".revust")];
            let mut qualified = None;
            for root in roots {
                if let Ok(root) = root.canonicalize()
                    && let Ok(relative) = source.strip_prefix(&root)
                    && relative.starts_with("extension-profiles")
                {
                    qualified = Some((root, relative.to_path_buf()));
                    break;
                }
            }
            let (root, relative) =
                qualified.ok_or("revust.config: structured profile escaped qualified layers")?;
            let bytes = contract::bounded_read(&contract::contained(&root, &relative)?)?;
            resources.insert(PathBuf::from(".revust").join(&relative), bytes);
            extensions["profiles"] = serde_json::json!({profile.clone():{"file":relative}});
        }
        let calibration = caller.join(".revust/profile.md");
        if calibration.try_exists().map_err(|e| e.to_string())? {
            let root = caller
                .join(".revust")
                .canonicalize()
                .map_err(|e| e.to_string())?;
            resources.insert(
                PathBuf::from(".revust/profile.md"),
                contract::bounded_read(&contract::contained(&root, Path::new("profile.md"))?)?,
            );
        }
        let mut config = String::new();
        for knob in input["legacy"]["knobs"]
            .as_array()
            .ok_or("revust.config: resolved ordinary settings missing")?
        {
            let name = knob["name"]
                .as_str()
                .ok_or("revust.config: unnamed ordinary setting")?;
            if name == "tasks-dir" {
                continue;
            }
            if !matches!(
                name,
                "profile"
                    | "auto-exit"
                    | "idle-timeout"
                    | "hard-timeout"
                    | "stagger-delay"
                    | "max-parallel"
                    | "verify-groups"
                    | "verify-group-by"
                    | "codex-sandbox"
            ) {
                return Err(format!(
                    "revust.config: unsupported resolved setting {name}"
                ));
            }
            let value = if let Some(value) = knob["value"].as_str() {
                value.to_owned()
            } else if knob["value"].is_number() || knob["value"].is_boolean() {
                knob["value"].to_string()
            } else {
                return Err("revust.config: ordinary value is not a scalar".into());
            };
            if value.chars().any(char::is_control) {
                return Err("revust.config: multiline ordinary setting refused".into());
            }
            config.push_str(&format!("{name} = {value}\n"));
        }
        resources.insert(PathBuf::from(".revust/config"), config.into_bytes());
        resources.insert(
            PathBuf::from(".revust/extensions.json"),
            serde_json::to_vec_pretty(&extensions).map_err(|e| e.to_string())?,
        );
        Self {
            directory: destination.to_path_buf(),
            profile,
            selected_profile: input["selected_profile"].clone(),
            policy,
            original_sha256: digest(effective),
            files: vec![],
            sha256: String::new(),
            original_budget,
            environment,
        }
        .save(resources, destination)
    }
    fn save(
        mut self,
        resources: std::collections::BTreeMap<PathBuf, Vec<u8>>,
        destination: &Path,
    ) -> Result<Self, String> {
        if resources.len() > 4096
            || resources.values().map(Vec::len).sum::<usize>() > 64 * 1024 * 1024
        {
            return Err("revust.config: resource bounds exceeded; no bytes omitted".into());
        }
        std::fs::create_dir(destination)
            .map_err(|e| format!("revust.config: fresh owned control directory required: {e}"))?;
        self.directory = destination.canonicalize().map_err(|e| e.to_string())?;
        let directory = &self.directory;
        let mut directories = BTreeSet::new();
        let mut files = Vec::new();
        for (relative, bytes) in resources {
            let path = directory.join(&relative);
            let parent = path
                .parent()
                .ok_or("revust.config: resource has no parent")?;
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let mut ancestor = parent;
            while ancestor.starts_with(directory.join(".revust")) {
                directories.insert(ancestor.to_path_buf());
                let Some(next) = ancestor.parent() else { break };
                ancestor = next;
            }
            use std::io::Write;
            use std::os::unix::fs::PermissionsExt;
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| e.to_string())?;
            file.set_permissions(std::fs::Permissions::from_mode(0o400))
                .map_err(|e| e.to_string())?;
            files.push(Artifact {
                path: relative,
                sha256: digest(&bytes),
            });
        }
        for directory in directories.iter().rev() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o500))
                .map_err(|e| e.to_string())?;
            std::fs::File::open(directory)
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
        self.files = files;
        self.sha256 = self.identity()?;
        self.verify()?;
        Ok(self)
    }
    /// Derive another serial round only from retained immutable resources. The
    /// original policy ceiling and raw inspection digest survive each allowance.
    pub fn with_allowance(&self, destination: &Path, allowance: u32) -> Result<Self, String> {
        self.verify()?;
        if allowance == 0 {
            return Err("revust.config: zero remaining allowance forbids launch".into());
        }
        let original = self.original_budget.as_ref().ok_or(
            "revust.config: legacy snapshot lacks original budget; parent reconciliation required",
        )?;
        let cap = original
            .max_calls
            .unwrap_or(allowance as u64)
            .min(allowance as u64);
        if cap == 0 {
            return Err("revust.config: original zero budget forbids launch".into());
        }
        let mut next = self.clone();
        next.policy["budget"]["max_calls"] = serde_json::json!(cap);
        let mut resources = std::collections::BTreeMap::new();
        for artifact in &self.files {
            let mut bytes =
                contract::bounded_read(&contract::contained(&self.directory, &artifact.path)?)?;
            if artifact.path == Path::new(".revust/extensions.json") {
                let mut extensions = json::read(&bytes)?;
                extensions["policy"] = next.policy.clone();
                bytes = serde_json::to_vec_pretty(&extensions).map_err(|e| e.to_string())?;
            }
            resources.insert(artifact.path.clone(), bytes);
        }
        next.save(resources, destination)
    }
    /// Literal execution preferences, separate from preserved source settings.
    /// The installed launcher must see an explicit flag so its fallback cannot
    /// replace the standing one-second close preference or a per-run keep-open.
    pub fn launcher_args(&self, keep_open: bool) -> Vec<std::ffi::OsString> {
        vec![
            "--config-dir".into(),
            self.config_dir().into_os_string(),
            "--profile".into(),
            self.profile.clone().into(),
            if keep_open {
                "--auto-exit=0s"
            } else {
                "--auto-exit=1s"
            }
            .into(),
        ]
    }
    /// The sole user/project layer passed explicitly to the installed engine.
    pub fn config_dir(&self) -> PathBuf {
        self.directory.join(".revust")
    }
    fn identity(&self) -> Result<String, String> {
        let legacy = (
            &self.directory,
            &self.profile,
            &self.selected_profile,
            &self.policy,
            &self.original_sha256,
            &self.files,
        );
        let bytes = if !self.environment.is_empty() {
            serde_json::to_vec(&(legacy, &self.original_budget, &self.environment))
        } else if let Some(original) = &self.original_budget {
            serde_json::to_vec(&(legacy, original))
        } else {
            serde_json::to_vec(&legacy)
        }
        .map_err(|e| e.to_string())?;
        Ok(digest(&bytes))
    }
    /// Verify retained resources, independent of today's profiles or provider accounts.
    pub fn verify(&self) -> Result<(), String> {
        if self.directory.canonicalize().map_err(|e| e.to_string())? != self.directory
            || self.identity()? != self.sha256
        {
            return Err("revust.config: saved identity changed".into());
        }
        let mut present = BTreeSet::new();
        config_files(&self.directory, Path::new(".revust"), &mut present, 0)?;
        if present
            != self
                .files
                .iter()
                .map(|a| a.path.clone())
                .collect::<BTreeSet<_>>()
            || present.len() != self.files.len()
        {
            return Err("revust.config: saved resource set changed".into());
        }
        for artifact in &self.files {
            if file_digest(&contract::contained(&self.directory, &artifact.path)?)?
                != artifact.sha256
            {
                return Err(format!(
                    "revust.config {}: saved bytes changed",
                    artifact.path.display()
                ));
            }
        }
        Ok(())
    }
}
pub(super) fn config_files(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    if depth > 32 || files.len() > 4096 {
        return Err("revust.config: saved resource bounds exceeded".into());
    }
    for entry in std::fs::read_dir(root.join(relative)).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = relative.join(entry.file_name());
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            config_files(root, &path, files, depth + 1)?;
        } else if ty.is_file() {
            if files.len() >= 4096 {
                return Err("revust.config: saved resource bounds exceeded".into());
            }
            files.insert(path);
        } else {
            return Err("revust.config: unexpected linked/special saved resource".into());
        }
    }
    Ok(())
}
