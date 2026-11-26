//! External instructions are captured as data; no installation or execution.
use super::contract::bounded_read;
use super::{SkillReference, SkillScope, digest};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// The actual source type, retained even when links are safely materialized.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// Ordinary regular file.
    File,
    /// Ordinary directory.
    Directory,
    /// Contained symbolic link to a file.
    LinkedFile,
    /// Contained symbolic link to a directory.
    LinkedDirectory,
}
/// One sorted resource identity in a pinned skill.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    /// Path relative to selected skill root.
    pub path: PathBuf,
    /// Original file type.
    pub kind: ResourceKind,
    /// Exact file-byte digest, or link spelling digest for a directory link.
    pub sha256: String,
    /// Original link spelling; all links must resolve inside selected root.
    pub link: Option<PathBuf>,
    /// Resolved target relative to the selected root, for safe snapshot links.
    pub target: Option<PathBuf>,
    /// Original Unix access bits; snapshot removes writes and retains execution.
    pub mode: u32,
}
/// External identity and immutable accessible resource directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedSkill {
    /// Qualified reference selected by the definition.
    pub reference: SkillReference,
    /// Actual winning scope; auto is resolved before pinning.
    pub scope: SkillScope,
    /// Canonical original source, including discovery alias resolution.
    pub source: PathBuf,
    /// Sorted complete directory manifest.
    pub resources: Vec<Resource>,
    /// Canonical manifest identity, also used as directory name.
    pub sha256: String,
    /// Run-owned read-only snapshot with original relative paths.
    pub directory: PathBuf,
}

fn unsafe_name(path: &Path) -> bool {
    path.components().any(|part| {
        part.as_os_str().to_str().is_some_and(|v| {
            v == ".env"
                || v.starts_with(".env.")
                || matches!(
                    v,
                    ".git"
                        | ".codex"
                        | ".claude"
                        | "auth.json"
                        | "credentials.json"
                        | "credentials"
                )
        })
    })
}
struct Capture {
    resources: Vec<Resource>,
    bytes: BTreeMap<PathBuf, Vec<u8>>,
    total: usize,
}
impl Capture {
    fn read(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let mut captured = Self {
            resources: Vec::new(),
            bytes: BTreeMap::new(),
            total: 0,
        };
        captured.walk(&root, Path::new(""), &mut BTreeSet::new(), 0)?;
        captured.resources.sort_by(|a, b| a.path.cmp(&b.path));
        if !captured.bytes.contains_key(Path::new("SKILL.md")) {
            return Err("SKILL.md: required instruction file missing".into());
        }
        for (relative, bytes) in &captured.bytes {
            if relative.extension().is_some_and(|ext| ext == "md") {
                let text = std::str::from_utf8(bytes)
                    .map_err(|_| format!("{}: Markdown is not UTF-8", relative.display()))?;
                validate_links(&root, relative, text)?;
            }
        }
        Ok(captured)
    }
    fn walk(
        &mut self,
        root: &Path,
        relative: &Path,
        stack: &mut BTreeSet<PathBuf>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 32 || self.resources.len() > 4096 {
            return Err(format!(
                "{}: skill resource bounds exceeded",
                relative.display()
            ));
        }
        let path = root.join(relative);
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("{}: dangling/unreadable resource: {e}", relative.display()))?;
        if !canonical.starts_with(root) {
            return Err(format!(
                "{}: resource link escapes selected skill root",
                relative.display()
            ));
        }
        if unsafe_name(canonical.strip_prefix(root).map_err(|e| e.to_string())?)
            || unsafe_name(relative)
        {
            return Err(format!(
                "{}: credential/provider resource is outside snapshot scope",
                relative.display()
            ));
        }
        let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        let link = meta
            .file_type()
            .is_symlink()
            .then(|| std::fs::read_link(&path))
            .transpose()
            .map_err(|e| e.to_string())?;
        let actual = std::fs::metadata(&canonical).map_err(|e| e.to_string())?;
        let target = link
            .as_ref()
            .map(|_| canonical.strip_prefix(root).unwrap().to_path_buf());
        let mode = actual.permissions().mode() & 0o777;
        if actual.is_dir() {
            if !stack.insert(canonical.clone()) {
                return Err(format!("{}: cyclic resource link", relative.display()));
            }
            if !relative.as_os_str().is_empty() {
                self.resources.push(Resource {
                    path: relative.into(),
                    kind: if link.is_some() {
                        ResourceKind::LinkedDirectory
                    } else {
                        ResourceKind::Directory
                    },
                    sha256: digest(
                        link.as_ref()
                            .map(|p| p.as_os_str().as_encoded_bytes())
                            .unwrap_or_default(),
                    ),
                    link,
                    target,
                    mode,
                });
            }
            for entry in std::fs::read_dir(&canonical).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                self.walk(root, &relative.join(entry.file_name()), stack, depth + 1)?;
            }
            stack.remove(&canonical);
        } else if actual.is_file() {
            let bytes = bounded_read(&canonical)?;
            self.total = self
                .total
                .checked_add(bytes.len())
                .ok_or("skill byte count overflow")?;
            if self.total > 64 * 1024 * 1024 {
                return Err("skill snapshot exceeds 64 MiB".into());
            }
            self.resources.push(Resource {
                path: relative.into(),
                kind: if link.is_some() {
                    ResourceKind::LinkedFile
                } else {
                    ResourceKind::File
                },
                sha256: digest(&bytes),
                link,
                target,
                mode,
            });
            self.bytes.insert(relative.into(), bytes);
        } else {
            return Err(format!(
                "{}: unsupported special resource",
                relative.display()
            ));
        }
        Ok(())
    }
}

fn validate_links(root: &Path, relative: &Path, text: &str) -> Result<(), String> {
    fn collect<'a>(node: &'a markdown::mdast::Node, links: &mut Vec<&'a str>) {
        use markdown::mdast::Node;
        match node {
            Node::Link(v) => links.push(&v.url),
            Node::Image(v) => links.push(&v.url),
            Node::Definition(v) => links.push(&v.url),
            _ => {}
        }
        if let Some(children) = node.children() {
            for child in children {
                collect(child, links);
            }
        }
    }
    let tree = markdown::to_mdast(text, &Default::default())
        .map_err(|e| format!("{}: {e}", relative.display()))?;
    let mut links = Vec::new();
    collect(&tree, &mut links);
    for target in links {
        if target.starts_with("https://")
            || target.starts_with("http://")
            || target.starts_with("mailto:")
            || target.starts_with('#')
        {
            continue;
        }
        let target = target.split(['#', '?']).next().unwrap_or("");
        if target.is_empty() {
            continue;
        }
        let mut decoded = Vec::new();
        let mut bytes = target.bytes();
        while let Some(b) = bytes.next() {
            if b == b'%' {
                let a = bytes.next().and_then(|c| (c as char).to_digit(16));
                let b = bytes.next().and_then(|c| (c as char).to_digit(16));
                let (Some(a), Some(b)) = (a, b) else {
                    return Err(format!("{}: malformed resource URL", relative.display()));
                };
                decoded.push((a * 16 + b) as u8);
            } else {
                decoded.push(b);
            }
        }
        let decoded = String::from_utf8(decoded)
            .map_err(|_| format!("{}: non-UTF-8 resource URL", relative.display()))?;
        let target = Path::new(&decoded);
        if target.is_absolute() || target.starts_with("~") || decoded.contains("://") {
            return Err(format!(
                "{}: external resource {} needs explicit scope",
                relative.display(),
                target.display()
            ));
        }
        let candidate = root
            .join(relative.parent().unwrap_or(Path::new("")))
            .join(target);
        let canonical = candidate.canonicalize().map_err(|e| {
            format!(
                "{}: referenced resource {} unavailable: {e}",
                relative.display(),
                target.display()
            )
        })?;
        if !canonical.starts_with(root) {
            return Err(format!(
                "{}: referenced resource {} escapes selected skill root",
                relative.display(),
                target.display()
            ));
        }
    }
    Ok(())
}

/// Resolve project/global precedence and snapshot all resources. Missing optional
/// instructions return a precise error too: the coordinator records that error as
/// a skip for optional references and as a dependent wait for required ones.
/// Call off the UI thread and only with host-qualified discovery roots.
pub fn snapshot_skill(
    reference: &SkillReference,
    project: &[PathBuf],
    global: &[PathBuf],
    destination: &Path,
) -> Result<PinnedSkill, String> {
    let choices = match reference.scope {
        SkillScope::Project => vec![(SkillScope::Project, project)],
        SkillScope::Global => vec![(SkillScope::Global, global)],
        SkillScope::Auto => vec![(SkillScope::Project, project), (SkillScope::Global, global)],
    };
    let mut selected = None;
    for (scope, roots) in choices {
        for discovery in roots {
            // A dangling root/skill is a gap, not permission to fall back to a
            // global skill with a different identity.
            if std::fs::symlink_metadata(discovery)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            {
                continue;
            }
            let root = discovery.canonicalize().map_err(|e| {
                format!("{}: discovery alias unavailable: {e}", discovery.display())
            })?;
            let path = root.join(&reference.name);
            if std::fs::symlink_metadata(&path)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            {
                continue;
            }
            let source = path.canonicalize().map_err(|e| {
                format!("{}: selected skill alias unavailable: {e}", path.display())
            })?;
            if !source.starts_with(&root) {
                return Err(format!(
                    "{}: selected skill alias escapes discovery scope",
                    path.display()
                ));
            }
            selected = Some((scope, source));
            break;
        }
        if selected.is_some() {
            break;
        }
    }
    let (scope, source) = selected.ok_or_else(|| {
        format!(
            "{} skill {}: not found in qualified {:?} discovery roots",
            if reference.required {
                "required"
            } else {
                "optional"
            },
            reference.name,
            reference.scope
        )
    })?;
    let captured = Capture::read(&source)?;
    let sha256 = digest(&serde_json::to_vec(&captured.resources).map_err(|e| e.to_string())?);
    std::fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    let directory = destination.join(&sha256);
    let pinned = PinnedSkill {
        reference: reference.clone(),
        scope,
        source,
        resources: captured.resources.clone(),
        sha256,
        directory,
    };
    if pinned.directory.try_exists().map_err(|e| e.to_string())? {
        pinned.verify()?;
        return Ok(pinned);
    }
    let scratch = destination.join(format!(".snapshot-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&scratch).map_err(|e| e.to_string())?;
    let result = (|| {
        let under_alias = |resource: &Resource| {
            captured.resources.iter().any(|other| {
                other.kind == ResourceKind::LinkedDirectory
                    && other.path != resource.path
                    && resource.path.starts_with(&other.path)
            })
        };
        // First create physical resources. Traversing a materialized directory
        // alias changes what `../resource` means, so retain safe internal links.
        for resource in &captured.resources {
            if under_alias(resource) {
                continue;
            }
            let path = scratch.join(&resource.path);
            match resource.kind {
                ResourceKind::Directory => {
                    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?
                }
                ResourceKind::File => {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    std::fs::write(&path, &captured.bytes[&resource.path])
                        .map_err(|e| e.to_string())?;
                }
                ResourceKind::LinkedFile | ResourceKind::LinkedDirectory => {}
            }
        }
        for resource in &captured.resources {
            if resource.link.is_some() && !under_alias(resource) {
                std::os::unix::fs::symlink(snapshot_link(resource)?, scratch.join(&resource.path))
                    .map_err(|e| e.to_string())?;
            }
        }
        // Re-read the entire source to detect edits during capture. No active
        // run will subsequently consult these mutable originals.
        if Capture::read(&pinned.source)?.resources != captured.resources {
            return Err("selected skill changed during snapshot capture".into());
        }
        std::fs::rename(&scratch, &pinned.directory).map_err(|e| e.to_string())?;
        let mut resources = captured.resources.iter().collect::<Vec<_>>();
        resources.sort_by_key(|r| std::cmp::Reverse(r.path.components().count()));
        for resource in resources {
            std::fs::set_permissions(
                pinned.directory.join(&resource.path),
                std::fs::Permissions::from_mode(resource.mode & 0o555),
            )
            .map_err(|e| e.to_string())?;
        }
        std::fs::set_permissions(&pinned.directory, std::fs::Permissions::from_mode(0o500))
            .map_err(|e| e.to_string())?;
        pinned.verify()
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&scratch);
    }
    result?;
    Ok(pinned)
}
impl PinnedSkill {
    /// Validate saved bytes without touching mutable original instructions.
    pub fn verify(&self) -> Result<(), String> {
        if std::fs::symlink_metadata(&self.directory)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("skill snapshot root became a symlink".into());
        }
        let actual = Capture::read(&self.directory)?;
        let normalized = self
            .resources
            .iter()
            .cloned()
            .map(|mut r| {
                if r.link.is_some() {
                    let link = snapshot_link(&r)?;
                    if r.kind == ResourceKind::LinkedDirectory {
                        r.sha256 = digest(link.as_os_str().as_encoded_bytes());
                    }
                    r.link = Some(link);
                }
                r.mode &= 0o555;
                Ok(r)
            })
            .collect::<Result<Vec<_>, String>>()?;
        if actual.resources != normalized {
            return Err(format!(
                "{}: pinned skill resources are corrupt or changed",
                self.directory.display()
            ));
        }
        Ok(())
    }
}

fn snapshot_link(resource: &Resource) -> Result<PathBuf, String> {
    let target = resource
        .target
        .as_ref()
        .ok_or("linked resource lacks a resolved target")?;
    let mut relative = PathBuf::new();
    for _ in resource.path.parent().unwrap_or(Path::new("")).components() {
        relative.push("..");
    }
    relative.push(target);
    Ok(relative)
}
