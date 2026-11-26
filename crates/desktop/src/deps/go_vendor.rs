//! Go owns mode selection and consistency; Guild reads only the validated vendor list.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{Answer, Dependency, Source, Stack, ask};

/// Ask using the caller's Go configuration. Only Go's specific refusal to
/// enumerate the module graph in vendor mode selects the manifest reader.
pub fn discover(root: &Path) -> Result<Answer, String> {
    match ask::run(root, Path::new("go"), &["list", "-m", "-json", "all"], ask::DEADLINE) {
        Ok(json) => Ok(super::go(&json)),
        Err(why) if why.ends_with("go: can't compute 'all' using the vendor directory \t(Use -mod=mod or -mod=readonly to bypass.)") => {
            // Validate again before reading: consistency errors must never be
            // converted into a successful (possibly empty) vendor answer.
            ask::run(root, Path::new("go"), &["list", "-m", "-json"], ask::DEADLINE)?;
            let context = context(root)?;
            let vendor = context.vendor()?;
            let manifest = vendor.join("modules.txt");
            let text = std::fs::read_to_string(&manifest)
                .map_err(|err| format!("{}: {err}", manifest.display()))?;
            read(&vendor, &text).map_err(|why| format!("{}: {why}", manifest.display()))
        }
        Err(why) => Err(why),
    }
}

#[derive(serde::Deserialize)]
struct Context {
    #[serde(rename = "GOMOD")]
    module: String,
    #[serde(rename = "GOWORK")]
    work: String,
}

impl Context {
    fn vendor(&self) -> Result<PathBuf, String> {
        let manifest = if !self.work.is_empty() && self.work != "off" {
            Path::new(&self.work)
        } else {
            Path::new(&self.module)
        };
        if !manifest.is_absolute() || manifest == Path::new("/dev/null") {
            return Err("Go did not report a module or workspace root".into());
        }
        manifest
            .parent()
            .map(|root| root.join("vendor"))
            .ok_or_else(|| "Go did not report a module or workspace root".into())
    }
}

fn context(root: &Path) -> Result<Context, String> {
    let json = ask::run(
        root,
        Path::new("go"),
        &["env", "-json", "GOMOD", "GOWORK"],
        ask::DEADLINE,
    )?;
    serde_json::from_str(&json).map_err(|err| format!("go env: {err}"))
}

/// Exact files whose contents can change Go's answer. Ancestor go.work files
/// also matter when created or removed; GOWORK may select a file elsewhere.
/// Runs on the dependency worker, including before a discovery that fails.
pub fn watched(root: &Path) -> Vec<PathBuf> {
    let mut paths: BTreeSet<PathBuf> = root.ancestors().map(|p| p.join("go.work")).collect();
    paths.extend([
        root.join("go.mod"),
        root.join("go.sum"),
        root.join("vendor/modules.txt"),
    ]);
    if let Ok(context) = context(root) {
        if let Ok(vendor) = context.vendor() {
            paths.insert(vendor.join("modules.txt"));
        }
        for path in [&context.module, &context.work] {
            if Path::new(path).is_absolute() && path != "/dev/null" {
                let path = PathBuf::from(path);
                let sum = if path.file_name().is_some_and(|name| name == "go.work") {
                    path.with_file_name("go.work.sum")
                } else {
                    path.with_extension("sum")
                };
                paths.extend([path, sum]);
            }
        }
    }
    paths.into_iter().collect()
}

#[derive(Default)]
struct Module {
    version: String,
    packages: BTreeSet<String>,
    listed: bool,
}

/// Read identities and package ownership from modules.txt, not the directory
/// prefixes: a module can be required without contributing a single package.
pub fn read(vendor: &Path, text: &str) -> Result<Answer, String> {
    let mut records: BTreeMap<(String, String), Module> = BTreeMap::new();
    let mut current: Option<(String, String)> = None;
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("## ") {
            if let Some(path) = &current {
                records.get_mut(path).unwrap().listed = true;
            }
            continue;
        }
        let bad = || format!("invalid vendor metadata at line {}: {line}", line_no + 1);
        if let Some(header) = line.strip_prefix("# ") {
            let fields: Vec<_> = header.split_whitespace().collect();
            let Some(path) = fields.first() else {
                return Err(bad());
            };
            if !safe_path(path) {
                return Err(bad());
            }
            let (version, rest) = match fields.get(1) {
                Some(version) if version.starts_with('v') => (*version, &fields[2..]),
                Some(&"=>") => ("", &fields[1..]),
                _ => return Err(bad()),
            };
            if !rest.is_empty() && !(rest[0] == "=>" && matches!(rest.len(), 2 | 3)) {
                return Err(bad());
            }
            // Versionless replacement records are trailers, not dependencies.
            // Versioned trailers repeat a header but carry no packages.
            current = if version.is_empty() {
                None
            } else {
                let key = ((*path).to_owned(), version.to_owned());
                records.entry(key.clone()).or_default().version = version.to_owned();
                Some(key)
            };
        } else {
            let Some(module) = current.as_ref() else {
                return Err(bad());
            };
            if !safe_path(line) || !Path::new(line).starts_with(&module.0) {
                return Err(bad());
            }
            let entry = records.get_mut(module).unwrap();
            entry.listed = true;
            entry.packages.insert(line.to_owned());
        }
    }
    // Unused version-specific replacements can name the same path at another
    // version. They are trailers, not conflicting selections or source roots.
    let mut modules = BTreeMap::new();
    for ((path, _), module) in records.into_iter().filter(|(_, module)| module.listed) {
        if modules.insert(path.clone(), module).is_some() {
            return Err(format!("multiple vendored versions of {path}"));
        }
    }
    // A package may belong only to its most specific declared module. Checking
    // this separately handles headers in either order and empty parent modules.
    for (path, module) in &modules {
        for package in &module.packages {
            if modules.keys().any(|other| {
                other != path
                    && Path::new(other).starts_with(path)
                    && Path::new(package).starts_with(other)
            }) {
                return Err(format!(
                    "package {package} crosses the module boundary of {path}"
                ));
            }
        }
    }
    let real_vendor = vendor
        .canonicalize()
        .map_err(|err| format!("{}: {err}", vendor.display()))?;
    let mut dependencies = Vec::new();
    for (path, module) in &modules {
        let source = if module.packages.is_empty() {
            Source::Missing
        } else {
            let root = vendor.join(path);
            checked_directory(&real_vendor, &root)?;
            for package in &module.packages {
                checked_directory(&real_vendor, &vendor.join(package))?;
            }
            let excluded = modules
                .keys()
                .filter_map(|other| {
                    (other != path)
                        .then(|| {
                            Path::new(other)
                                .strip_prefix(path)
                                .ok()
                                .map(Path::to_path_buf)
                        })
                        .flatten()
                })
                .collect();
            Source::Vendor { root, excluded }
        };
        dependencies.push(Dependency {
            stack: Stack::Go,
            label: format!("{path}@{}", module.version),
            source,
        });
    }
    Ok(Answer {
        jdks: Vec::new(),
        dependencies,
    }
    .sorted())
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.chars().any(|c| c.is_whitespace() || c.is_control())
        })
}

fn checked_directory(vendor: &Path, path: &Path) -> Result<(), String> {
    let real = path
        .canonicalize()
        .map_err(|err| format!("vendored package {}: {err}", path.display()))?;
    if !real.starts_with(vendor) || !real.is_dir() {
        return Err(format!(
            "vendored package {} is outside vendor or not a directory",
            path.display()
        ));
    }
    Ok(())
}
