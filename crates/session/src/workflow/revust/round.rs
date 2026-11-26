//! Installed SDK preparation and an immutable, source-qualified launcher intent.
use super::*;
use crate::workflow::{ContractSnapshot, SourceIdentity, contract, json};
use std::{ffi::OsString, io::Write, os::unix::fs::PermissionsExt};

/// Exact parent-owned scope, distinct from the caller directory selecting config.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevustSubject {
    /// Workflow-owned external archive root, never the user's legacy history root.
    pub tasks_dir: PathBuf,
    /// Stable task name, shared by correction rounds.
    pub task: String,
    /// Unique serial round name.
    pub round: String,
    /// Actual reviewed checkout/dirty/untracked identity.
    pub source: SourceIdentity,
    /// Accepted saved artifact bytes, accessible even after original changes.
    pub contract: ContractSnapshot,
    /// Complete literal parent review subject and boundaries.
    pub scope: String,
    /// Explicit per-run request to leave completed presentation open.
    pub keep_open: bool,
}
/// A prepared external round. This is not a launch/usage receipt or a verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevustRound {
    /// Selected installed route and all executable/helper identities.
    pub package: RevustPackage,
    /// Frozen profile/policy and raw winning source resources.
    pub configuration: RevustConfigSnapshot,
    /// Exact requested scope and source.
    pub subject: RevustSubject,
    /// SDK-created actual archive directory, not an invented job ID.
    pub archive: PathBuf,
    /// Literal selected launcher argv; never shell source.
    pub args: Vec<String>,
    /// Exact immutable authored inputs inside the archive.
    pub inputs: Vec<Artifact>,
    /// Actual installed dry-run output identity; dry-run starts no provider.
    pub dry_run_sha256: String,
    /// Aggregate request identity, checked before dependent execution.
    pub sha256: String,
    /// Value-free private resource owner; never the configuration values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<RevustEnvironmentLease>,
}
struct PreparedEnvironment(Option<RevustEnvironmentLease>);
impl Drop for PreparedEnvironment {
    fn drop(&mut self) {
        if let Some(lease) = &self.0
            && let Err(error) = lease.release()
        {
            tracing::warn!(%error,owner=%lease.owner,path=%lease.directory.display(),"private review environment retained after preparation failure");
        }
    }
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
fn input(
    root: &Path,
    path: &Path,
    bytes: &[u8],
    manifest: &mut Vec<Artifact>,
) -> Result<(), String> {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().ok_or("revust.round: input parent missing")?)
        .map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&full)
        .map_err(|e| format!("revust.round input {}: {e}", path.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    file.set_permissions(std::fs::Permissions::from_mode(0o400))
        .map_err(|e| e.to_string())?;
    manifest.push(Artifact {
        path: path.to_path_buf(),
        sha256: digest(bytes),
    });
    Ok(())
}
impl RevustPackage {
    /// Prepare only a new owned round through the actual selected SDK. `new`,
    /// config inspection and `dry-run` do not launch a review/provider. A host
    /// must reserve aggregate allowance and recheck authority before execution.
    pub fn prepare_round(
        &self,
        configuration: &RevustConfigSnapshot,
        subject: RevustSubject,
    ) -> Result<RevustRound, String> {
        self.verify()?;
        configuration.verify()?;
        subject.source.verify()?;
        subject.contract.verify()?;
        if !name(&subject.task)
            || !name(&subject.round)
            || subject.scope.trim().is_empty()
            || subject.scope.len() > 128 * 1024
            || subject.scope.contains('\0')
        {
            return Err(
                "revust.round: bounded literal scope and contained task/round identities required"
                    .into(),
            );
        }
        if !subject.tasks_dir.is_absolute() {
            return Err("revust.round: absolute owned tasks root required".into());
        }
        let archive = subject.tasks_dir.join(&subject.task).join(&subject.round);
        if archive.try_exists().map_err(|e| e.to_string())? {
            return Err("revust.round: existing archive retained; reconcile instead of preparing another launch".into());
        }
        std::fs::create_dir_all(&subject.tasks_dir).map_err(|e| e.to_string())?;
        let tasks = subject
            .tasks_dir
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if tasks != subject.tasks_dir {
            return Err("revust.round: tasks root must be qualified canonical".into());
        }
        let mut environment = PreparedEnvironment(RevustEnvironmentLease::create(
            configuration,
            &subject.round,
        )?);
        if let Some(lease) = &environment.0 {
            let bytes = serde_json::to_vec_pretty(lease).map_err(|e| e.to_string())?;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(configuration.directory.join("environment-lease.json"))
                .map_err(|e| {
                    format!("revust.environment: persist owner before preparation: {e}")
                })?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|_| "revust.environment: owner receipt synchronization failed")?;
            std::fs::File::open(&configuration.directory)
                .and_then(|f| f.sync_all())
                .map_err(|_| "revust.environment: owner directory synchronization failed")?;
        }
        let mut args = configuration.launcher_args(subject.keep_open);
        if let Some(lease) = &environment.0 {
            args[1] = lease.directory.as_os_str().into();
        }

        args.extend([
            OsString::from("--workdir"),
            subject.source.target.as_os_str().into(),
            "--tasks-dir".into(),
            tasks.into_os_string(),
            "--task".into(),
            subject.task.clone().into(),
            "--run".into(),
            subject.round.clone().into(),
        ]);
        let mut prepare = vec![OsString::from("new")];
        prepare.extend(args.clone());
        self.preparation(&configuration.directory, &prepare)?;
        if archive.canonicalize().map_err(|e| e.to_string())? != archive {
            return Err("revust.round: SDK archive escaped owned root".into());
        }
        let mut inputs = Vec::new();
        input(
            &archive,
            Path::new("input/scope.md"),
            subject.scope.as_bytes(),
            &mut inputs,
        )?;
        input(
            &archive,
            Path::new("input/context/workflow.json"),
            &serde_json::to_vec_pretty(&subject).map_err(|e| e.to_string())?,
            &mut inputs,
        )?;
        let mut total = subject.scope.len();
        for artifact in &subject.contract.contract.artifacts {
            let bytes = contract::bounded_read(&contract::contained(
                &subject.contract.directory,
                &artifact.path,
            )?)?;
            total = total
                .checked_add(bytes.len())
                .ok_or("revust.round: context size overflow")?;
            if total > 64 * 1024 * 1024 || inputs.len() >= 4096 {
                return Err("revust.round: context bounds exceeded; no dependent execution".into());
            }
            input(
                &archive,
                &PathBuf::from("input/context/contract").join(&artifact.path),
                &bytes,
                &mut inputs,
            )?;
        }
        let calibration = configuration.config_dir().join("profile.md");
        if calibration.try_exists().map_err(|e| e.to_string())? {
            input(
                &archive,
                Path::new("input/profile.md"),
                &contract::bounded_read(&calibration)?,
                &mut inputs,
            )?;
        }
        let mut dry = vec![OsString::from("dry-run")];
        dry.extend(args.clone());
        let output = self.preparation(&configuration.directory, &dry)?;
        json::read(&output)?;
        inputs.sort_by(|a, b| a.path.cmp(&b.path));
        let args = args
            .into_iter()
            .map(|s| {
                s.into_string()
                    .map_err(|_| "revust.round: non-UTF8 launcher path is unsupported".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut round = RevustRound {
            package: self.clone(),
            configuration: configuration.clone(),
            subject,
            archive,
            args,
            inputs,
            dry_run_sha256: digest(&output),
            sha256: String::new(),
            environment: environment.0.clone(),
        };
        round.sha256 = round.identity()?;
        round.verify()?;
        environment.0.take(); // Ownership is now recorded by the prepared round.
        Ok(round)
    }
}
impl RevustRound {
    fn identity(&self) -> Result<String, String> {
        let original = (
            &self.package,
            &self.configuration,
            &self.subject,
            &self.archive,
            &self.args,
            &self.inputs,
            &self.dry_run_sha256,
        );
        let bytes = if let Some(environment) = &self.environment {
            serde_json::to_vec(&(original, environment))
        } else {
            serde_json::to_vec(&original)
        }
        .map_err(|e| e.to_string())?;
        Ok(digest(&bytes))
    }
    /// Verify retained scope/resources and selected executable identities. The
    /// owning coordinator must additionally recapture actual source before launch.
    pub fn verify(&self) -> Result<(), String> {
        if self.identity()? != self.sha256 {
            return Err("revust.round: frozen request changed".into());
        }
        self.package.verify()?;
        self.configuration.verify()?;
        self.subject.source.verify()?;
        self.subject.contract.verify()?;
        if self.archive.canonicalize().map_err(|e| e.to_string())? != self.archive
            || self.archive
                != self
                    .subject
                    .tasks_dir
                    .join(&self.subject.task)
                    .join(&self.subject.round)
        {
            return Err("revust.round: archive identity changed".into());
        }
        for artifact in &self.inputs {
            if file_digest(&contract::contained(&self.archive, &artifact.path)?)? != artifact.sha256
            {
                return Err(format!(
                    "revust.round input {}: saved bytes changed",
                    artifact.path.display()
                ));
            }
        }
        let mut actual = BTreeSet::new();
        super::config::config_files(&self.archive, Path::new("input"), &mut actual, 0)?;
        if actual != self.inputs.iter().map(|a| a.path.clone()).collect()
            || actual.len() != self.inputs.len()
        {
            return Err("revust.round: input resources changed".into());
        }
        Ok(())
    }
}
