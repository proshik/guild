//! Selected external package identity. Qualification executes no review/provider.
use super::{Artifact, digest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
};

/// Stable selected route plus exact installed launcher/binary/helper identities.
/// Invocation retains the selected route, rather than hardcoding a Cellar version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevustPackage {
    /// User-selected absolute package selector, preserved literally.
    pub route: PathBuf,
    /// Qualified canonical package root; a selector change requires reconciliation.
    pub root: PathBuf,
    /// Installed distribution version from its build receipt.
    pub version: String,
    /// Sorted package-relative exact binary/build/launcher helper bytes.
    pub files: Vec<Artifact>,
    /// Aggregate identity of route, canonical root, version and file manifest.
    pub sha256: String,
}
fn file_digest(path: &Path) -> Result<String, String> {
    if !std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err(format!(
            "revust.package {}: expected regular file",
            path.display()
        ));
    }
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("revust.package {}: {e}", path.display()))?;
    let mut hash = Sha256::new();
    let mut block = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut block).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&block[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn scripts(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
    seen: &mut BTreeSet<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    if depth > 16 || files.len() > 4096 {
        return Err("revust.package: helper resource bounds exceeded".into());
    }
    let directory = super::contract::contained(root, relative)?;
    if !seen.insert(directory.clone()) {
        return Err("revust.package: cyclic/aliased helper directories".into());
    }
    for entry in std::fs::read_dir(&directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_name() == "__pycache__" {
            continue;
        }
        let child = relative.join(entry.file_name());
        let path = super::contract::contained(root, &child)?;
        if path.is_dir() {
            scripts(root, &child, files, seen, depth + 1)?;
        } else if path.is_file() {
            if files.len() >= 4096 {
                return Err("revust.package: helper resource bounds exceeded".into());
            }
            files.insert(child);
        } else {
            return Err("revust.package: special helper resource is unsupported".into());
        }
    }
    seen.remove(&directory);
    Ok(())
}
impl RevustPackage {
    /// Qualify an installed selector using its build receipt, exact executable
    /// digest and complete helper tree. Never installs, rewrites or runs it.
    pub fn qualify(route: &Path) -> Result<Self, String> {
        if !route.is_absolute() {
            return Err("revust.package: absolute selected route required".into());
        }
        let root = route
            .canonicalize()
            .map_err(|e| format!("revust.package selector: {e}"))?;
        let build = super::contract::contained(&root, Path::new("build.json"))?;
        let build = super::json::read(&super::contract::bounded_read(&build)?)?;
        let version = build["version"]
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 128 && !v.chars().any(char::is_control))
            .ok_or("revust.package: distribution version missing")?
            .to_owned();
        let mut paths = BTreeSet::from([PathBuf::from("bin/revust"), PathBuf::from("build.json")]);
        scripts(
            &root,
            Path::new("scripts"),
            &mut paths,
            &mut BTreeSet::new(),
            0,
        )?;
        for required in [
            "scripts/launch-revust.sh",
            "scripts/host-launch.py",
            "scripts/guild-launch.py",
            "scripts/guild-run.sh",
            "scripts/helper_common.py",
        ] {
            if !paths.contains(Path::new(required)) {
                return Err(format!(
                    "revust.package: required Guild launcher helper {required} missing"
                ));
            }
        }
        let files = paths
            .into_iter()
            .map(|path| {
                Ok(Artifact {
                    sha256: file_digest(&super::contract::contained(&root, &path)?)?,
                    path,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let binary = files
            .iter()
            .find(|a| a.path == Path::new("bin/revust"))
            .ok_or("revust.package: binary missing")?;
        if build["binary_sha256"].as_str() != Some(&binary.sha256) {
            return Err("revust.package: executable differs from installed build receipt".into());
        }
        let sha256 = digest(
            &serde_json::to_vec(&(route, &root, &version, &files)).map_err(|e| e.to_string())?,
        );
        let package = Self {
            route: route.to_path_buf(),
            root,
            version,
            files,
            sha256,
        };
        package.verify()?;
        Ok(package)
    }
    /// Absolute stable selected binary path, not a PATH lookup or versioned alias.
    pub fn program(&self) -> PathBuf {
        self.route.join("bin/revust")
    }
    /// Selected installed launcher; host/provider choice is never substituted.
    pub fn launcher(&self) -> PathBuf {
        self.route.join("scripts/launch-revust.sh")
    }
    /// Recheck the same selector and every byte before a dependent invocation.
    pub fn verify(&self) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        if self.route.canonicalize().map_err(|e| e.to_string())? != self.root
            || digest(
                &serde_json::to_vec(&(&self.route, &self.root, &self.version, &self.files))
                    .map_err(|e| e.to_string())?,
            ) != self.sha256
        {
            return Err(
                "revust.package: selected route/manifest changed; dependent execution withheld"
                    .into(),
            );
        }
        let mut actual = BTreeSet::from([PathBuf::from("bin/revust"), PathBuf::from("build.json")]);
        scripts(
            &self.root,
            Path::new("scripts"),
            &mut actual,
            &mut BTreeSet::new(),
            0,
        )?;
        if actual
            != self
                .files
                .iter()
                .map(|a| a.path.clone())
                .collect::<BTreeSet<_>>()
            || actual.len() != self.files.len()
        {
            return Err("revust.package: helper artifact set changed".into());
        }
        for artifact in &self.files {
            if file_digest(&super::contract::contained(&self.root, &artifact.path)?)?
                != artifact.sha256
            {
                return Err(format!(
                    "revust.package {}: installed bytes changed",
                    artifact.path.display()
                ));
            }
        }
        for executable in [self.program(), self.launcher()] {
            if std::fs::metadata(&executable)
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o111
                == 0
            {
                return Err(format!(
                    "revust.package {}: not executable",
                    executable.display()
                ));
            }
        }
        Ok(())
    }
}

mod config;
pub use config::RevustConfigSnapshot;
mod probe;

mod round;
pub use round::{RevustRound, RevustSubject};

mod launcher;
pub use launcher::{RevustHost, RevustLauncher, RevustLauncherExit};

mod environment;
pub use environment::{EnvironmentReference, RevustEnvironmentLease};
