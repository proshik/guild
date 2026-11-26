//! What a project depends on, by what its build system resolved — M8.14.
//!
//! *External Libraries* is filled by the build system: Gradle's
//! import lists `Gradle: ch.qos.logback:logback-classic:1.5.21` and the
//! two hundred others, and the project's JDK. Guild's listed only the
//! roots a person had `f12`'d into, which was M8.8.3's decision 15 and is
//! retired by decision 41 of the spec.
//!
//! **A stack is a row of data**: the files that say it is there, and a
//! reader of what it answers. Four answers come from asking the tool —
//! Gradle, Maven, Cargo, Go — and three are files the tool already wrote —
//! `node_modules`, `Gemfile.lock`, `mix.lock` — because running `npm ls` or
//! `bundle` adds a process that fails for reasons of its own and says
//! nothing the files do not. Measured in the plan's task 0.
//!
//! The readers are here and nothing runs here: `root::dependencies` asks,
//! on a thread, and hands the answers to these.

use std::path::{Path, PathBuf};

pub mod ask;
pub mod go_vendor;

/// A build system Guild knows how to ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stack {
    Gradle,
    Maven,
    Cargo,
    Go,
    Npm,
    Bundler,
    Mix,
}

impl Stack {
    /// Every stack, in the order a project's are asked.
    pub const ALL: [Stack; 7] = [
        Stack::Gradle,
        Stack::Maven,
        Stack::Cargo,
        Stack::Go,
        Stack::Npm,
        Stack::Bundler,
        Stack::Mix,
    ];

    /// The files at a project's root that say this stack is there. Any
    /// one of them is enough.
    pub fn manifests(self) -> &'static [&'static str] {
        match self {
            Stack::Gradle => &[
                "settings.gradle.kts",
                "settings.gradle",
                "build.gradle.kts",
                "build.gradle",
            ],
            Stack::Maven => &["pom.xml"],
            Stack::Cargo => &["Cargo.toml"],
            Stack::Go => &["go.mod", "go.work"],
            Stack::Npm => &["package.json"],
            Stack::Bundler => &["Gemfile.lock"],
            Stack::Mix => &["mix.exs"],
        }
    }

    /// The files whose change means the answer may have changed: the
    /// manifests, and what the tool writes when it resolves.
    pub fn watched(self) -> &'static [&'static str] {
        match self {
            Stack::Gradle => &[
                "settings.gradle.kts",
                "settings.gradle",
                "build.gradle.kts",
                "build.gradle",
                "gradle.properties",
                "libs.versions.toml",
            ],
            Stack::Maven => &["pom.xml"],
            Stack::Cargo => &["Cargo.toml", "Cargo.lock"],
            Stack::Go => &[
                "go.mod",
                "go.sum",
                "go.work",
                "go.work.sum",
                "vendor/modules.txt",
            ],
            Stack::Npm => &[
                "package.json",
                "package-lock.json",
                "pnpm-lock.yaml",
                "yarn.lock",
            ],
            Stack::Bundler => &["Gemfile", "Gemfile.lock"],
            Stack::Mix => &["mix.exs", "mix.lock"],
        }
    }

    /// The stacks a project at `root` uses, by the files at its root.
    pub fn of(root: &Path) -> Vec<Stack> {
        Stack::ALL
            .into_iter()
            .filter(|stack| {
                stack
                    .manifests()
                    .iter()
                    .any(|name| root.join(name).is_file())
            })
            .collect()
    }
}

/// One thing a project depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// Which build system said so.
    pub stack: Stack,
    /// What the row says — the name the stack's own IDE gives it.
    pub label: String,
    /// Where it can be read.
    pub source: Source,
}

/// Where a dependency's code is on the disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A directory of sources — a crate, a module, a package, a gem.
    Dir(PathBuf),
    /// A vendored module. Nested module roots belong to separate libraries.
    Vendor {
        root: PathBuf,
        excluded: Vec<PathBuf>,
    },
    /// A jar of classes. Its sources are the `-sources.jar` beside it, if
    /// one was downloaded — `jars::sources_beside` finds it.
    Jar(PathBuf),
    /// Resolved and not on the disk: a Go module not yet downloaded, a
    /// gem the lockfile names and no Ruby has installed.
    Missing,
}

impl Source {
    /// A source directory, if it can be opened without extracting an archive.
    pub fn directory(&self) -> Option<&Path> {
        match self {
            Self::Dir(root) | Self::Vendor { root, .. } => Some(root),
            _ => None,
        }
    }

    /// The source browser, preserving module boundaries in a shared vendor tree.
    pub fn open_tree(&self) -> Option<crate::tree::Tree> {
        match self {
            Self::Dir(root) => Some(crate::tree::Tree::open(root)),
            Self::Vendor { root, excluded } => Some(crate::tree::Tree::scoped(root, excluded)),
            _ => None,
        }
    }
}

/// A JDK a project builds with — labelled `< 21 >`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jdk {
    /// What the row says.
    pub label: String,
    /// Its home; the sources are `lib/src.zip` under it.
    pub home: PathBuf,
}

impl Jdk {
    /// The JDK's own sources, as a zip laid out by module.
    pub fn sources(&self) -> PathBuf {
        self.home.join("lib").join("src.zip")
    }
}

/// Everything one project depends on, sorted the familiar way: the JDKs
/// first, then the libraries by what they say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    pub jdks: Vec<Jdk>,
    pub dependencies: Vec<Dependency>,
}

impl Answer {
    /// Two answers as one — a project with two stacks — each dependency
    /// once, in that order.
    pub fn merge(mut self, other: Answer) -> Answer {
        for jdk in other.jdks {
            if !self.jdks.contains(&jdk) {
                self.jdks.push(jdk);
            }
        }
        for dependency in other.dependencies {
            if !self
                .dependencies
                .iter()
                .any(|known| known.label == dependency.label)
            {
                self.dependencies.push(dependency);
            }
        }
        self.sorted()
    }

    /// JDKs first, then the libraries.
    pub fn sorted(mut self) -> Answer {
        self.jdks.sort_by(|a, b| a.label.cmp(&b.label));
        self.dependencies
            .sort_by_key(|dependency| dependency.label.to_lowercase());
        self
    }
}

/// Gradle, through Guild's init script: `GUILD-DEP` and `GUILD-JDK`
/// lines, and anything else Gradle prints ignored.
///
/// A multi-project build prints a dependency once per project that has
/// it; it is one row.
pub fn gradle(output: &str) -> Answer {
    let mut answer = Answer::default();
    for line in output.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["GUILD-DEP", coordinates, jar] => {
                let label = format!("Gradle: {coordinates}");
                if !answer.dependencies.iter().any(|known| known.label == label) {
                    answer.dependencies.push(Dependency {
                        stack: Stack::Gradle,
                        label,
                        source: Source::Jar(PathBuf::from(jar)),
                    });
                }
            }
            ["GUILD-JDK", version, home] => {
                let jdk = Jdk {
                    label: format!("< {version} >"),
                    home: PathBuf::from(home),
                };
                if !answer.jdks.contains(&jdk) {
                    answer.jdks.push(jdk);
                }
            }
            _ => {}
        }
    }
    answer.sorted()
}

/// Maven, through `dependency:list` with absolute file names:
/// `   group:artifact:type[:classifier]:version:scope:/path/to.jar`,
/// with ` -- module name` after it on a modular jar.
///
/// **Maven colours its list even into a file**, measured: the path ended
/// in `\x1b[36m`. The colour is taken out before anything is read, and
/// `ask` runs it with `-B` besides.
pub fn maven(output: &str) -> Answer {
    let mut answer = Answer::default();
    for line in output.lines() {
        let line = without_colour(line);
        let line = line.trim();
        let line = line.split(" -- ").next().unwrap_or(line);
        let Some(at) = line.find(":/") else {
            continue;
        };
        let (coordinates, path) = (&line[..at], &line[at + 1..]);
        let fields: Vec<&str> = coordinates.split(':').collect();
        // group, artifact, type, [classifier], version, scope
        if fields.len() < 5 {
            continue;
        }
        let version = fields[fields.len() - 2];
        let label = format!("Maven: {}:{}:{version}", fields[0], fields[1]);
        if !answer.dependencies.iter().any(|known| known.label == label) {
            answer.dependencies.push(Dependency {
                stack: Stack::Maven,
                label,
                source: Source::Jar(PathBuf::from(path)),
            });
        }
    }
    answer.sorted()
}

/// Cargo, through `cargo metadata --format-version 1`: every package that
/// is not a member of the workspace, at the directory of its manifest.
pub fn cargo(json: &str) -> Answer {
    let Ok(metadata) = serde_json::from_str::<serde_json::Value>(json) else {
        return Answer::default();
    };
    let members: Vec<&str> = metadata["workspace_members"]
        .as_array()
        .map(|ids| ids.iter().filter_map(|id| id.as_str()).collect())
        .unwrap_or_default();
    let dependencies = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|package| {
            package["id"]
                .as_str()
                .is_some_and(|id| !members.contains(&id))
        })
        .filter_map(|package| {
            let name = package["name"].as_str()?;
            let version = package["version"].as_str()?;
            let manifest = PathBuf::from(package["manifest_path"].as_str()?);
            Some(Dependency {
                stack: Stack::Cargo,
                label: format!("{name} {version}"),
                source: Source::Dir(manifest.parent()?.to_path_buf()),
            })
        })
        .collect();
    Answer {
        jdks: Vec::new(),
        dependencies,
    }
    .sorted()
}

/// Go, through `go list -m -json all`: a stream of objects, the main
/// module among them. A module not yet downloaded has no `Dir`; a
/// replaced one is read where the replacement is.
pub fn go(json: &str) -> Answer {
    let dependencies = serde_json::Deserializer::from_str(json)
        .into_iter::<serde_json::Value>()
        .filter_map(Result::ok)
        .filter(|module| !module["Main"].as_bool().unwrap_or(false))
        .filter_map(|module| {
            let path = module["Path"].as_str()?;
            let version = module["Version"].as_str().unwrap_or_default();
            let dir = module["Replace"]["Dir"]
                .as_str()
                .or_else(|| module["Dir"].as_str());
            Some(Dependency {
                stack: Stack::Go,
                label: if version.is_empty() {
                    path.to_owned()
                } else {
                    format!("{path}@{version}")
                },
                source: dir.map_or(Source::Missing, |dir| Source::Dir(PathBuf::from(dir))),
            })
        })
        .collect();
    Answer {
        jdks: Vec::new(),
        dependencies,
    }
    .sorted()
}

/// npm, pnpm and yarn alike: what `node_modules` holds at its top, a
/// scope's packages one level down. pnpm's are symlinks into `.pnpm`, and
/// reading through them is what reaches the package.
pub fn npm(root: &Path) -> Answer {
    let modules = root.join("node_modules");
    let mut dependencies = Vec::new();
    let mut read = |dir: &Path| {
        let Ok(text) = std::fs::read_to_string(dir.join("package.json")) else {
            return;
        };
        let Ok(package) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        let (Some(name), Some(version)) = (package["name"].as_str(), package["version"].as_str())
        else {
            return;
        };
        dependencies.push(Dependency {
            stack: Stack::Npm,
            label: format!("{name}@{version}"),
            source: Source::Dir(dir.to_path_buf()),
        });
    };
    for entry in children(&modules) {
        let name = entry.file_name().map(|n| n.to_string_lossy().into_owned());
        match name.as_deref() {
            Some(name) if name.starts_with('.') => {}
            Some(name) if name.starts_with('@') => {
                for scoped in children(&entry) {
                    read(&scoped);
                }
            }
            Some(_) => read(&entry),
            None => {}
        }
    }
    Answer {
        jdks: Vec::new(),
        dependencies,
    }
    .sorted()
}

/// Bundler, by `Gemfile.lock`: every spec it names, found in the first of
/// `gem_roots` that holds `gems/<name>-<version>`. The project's own
/// `vendor/bundle` is looked in first, as Bundler does.
///
/// **The lockfile and not `bundle`**, because the `bundle` on a login
/// `PATH` may be another Ruby's and refuse the lockfile outright —
/// measured in task 0.
pub fn bundler(root: &Path, gem_roots: &[PathBuf]) -> Answer {
    let Ok(lock) = std::fs::read_to_string(root.join("Gemfile.lock")) else {
        return Answer::default();
    };
    let mut roots: Vec<PathBuf> = children(&root.join("vendor/bundle/ruby"));
    roots.extend(gem_roots.iter().cloned());

    let mut dependencies = Vec::new();
    for line in lock.lines() {
        // A spec is indented four; what it needs is indented six.
        let Some(spec) = line.strip_prefix("    ") else {
            continue;
        };
        if spec.starts_with(' ') {
            continue;
        }
        let Some((name, rest)) = spec.split_once(" (") else {
            continue;
        };
        let Some(version) = rest.strip_suffix(')') else {
            continue;
        };
        let label = format!("{name} ({version})");
        if dependencies
            .iter()
            .any(|known: &Dependency| known.label == label)
        {
            continue;
        }
        let found = roots
            .iter()
            .map(|gems| gems.join("gems").join(format!("{name}-{version}")))
            .find(|dir| dir.is_dir());
        dependencies.push(Dependency {
            stack: Stack::Bundler,
            label,
            source: found.map_or(Source::Missing, Source::Dir),
        });
    }
    Answer {
        jdks: Vec::new(),
        dependencies,
    }
    .sorted()
}

/// Mix, by `mix.lock`: each entry's version — a Hex package's, or a git
/// dependency's commit — and its sources in the project's `deps/`.
pub fn mix(root: &Path) -> Answer {
    let Ok(lock) = std::fs::read_to_string(root.join("mix.lock")) else {
        return Answer::default();
    };
    let mut dependencies = Vec::new();
    for line in lock.lines() {
        // `  "jason": {:hex, :jason, "1.4.4", "…", …},`
        // `  "thing": {:git, "https://…", "0123abcd…", […]},`
        let line = line.trim();
        let Some(rest) = line.strip_prefix('"') else {
            continue;
        };
        let Some((name, rest)) = rest.split_once("\": {") else {
            continue;
        };
        let quoted: Vec<&str> = rest.split('"').skip(1).step_by(2).collect();
        let version = if rest.starts_with(":hex") {
            quoted.first().map(|v| (*v).to_owned())
        } else if rest.starts_with(":git") {
            quoted.get(1).map(|sha| sha.chars().take(7).collect())
        } else {
            None
        };
        let Some(version) = version else {
            continue;
        };
        let dir = root.join("deps").join(name);
        dependencies.push(Dependency {
            stack: Stack::Mix,
            label: format!("{name} {version}"),
            source: if dir.is_dir() {
                Source::Dir(dir)
            } else {
                Source::Missing
            },
        });
    }
    Answer {
        jdks: Vec::new(),
        dependencies,
    }
    .sorted()
}

/// A line without its ANSI colour codes — `ESC [ … m`.
fn without_colour(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            for inside in chars.by_ref() {
                if inside.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// A directory's entries, sorted, and nothing for one that is not there.
fn children(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    entries.sort();
    entries
}
