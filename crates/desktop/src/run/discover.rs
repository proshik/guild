//! Bounded source-only discovery, shared with editor declarations.
//! No project executable or task template is evaluated here.

use std::path::Path;

pub fn relevant(path: &Path) -> bool {
    super::source::Runner::of(path).is_some()
        || path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            matches!(
                n,
                "package-lock.json"
                    | "npm-shrinkwrap.json"
                    | "pnpm-lock.yaml"
                    | "yarn.lock"
                    | "bun.lock"
                    | "bun.lockb"
                    | ".gitignore"
            )
        })
}

/// A source of commands: a file, and what it declares.
pub struct Runner {
    /// What must sit beside the project for this row to apply.
    pub file: &'static str,
    /// The lines it declares, from the project's directory.
    pub list: fn(&Path) -> Vec<String>,
}

/// Every runner, in the order their commands are offered.
pub const RUNNERS: &[Runner] = &[
    Runner {
        file: "justfile",
        list: just,
    },
    Runner {
        file: "package.json",
        list: npm,
    },
];

/// Source-only bounded discovery. The window calls this off-thread.
pub fn declared(project: &Path) -> Vec<String> {
    tasks(project)
        .into_iter()
        .filter_map(|task| {
            let line = task.command(&[], None).ok()?;
            let parent = task.identity.source.parent().unwrap_or(Path::new(""));
            Some(if parent.as_os_str().is_empty() {
                line
            } else {
                format!(
                    "cd {} && {line}",
                    super::source::shell_line(&[parent.to_string_lossy().into_owned()])
                )
            })
        })
        .collect()
}

pub fn tasks(project: &Path) -> Vec<super::source::Task> {
    catalogue(project).tasks
}

#[derive(Default)]
pub struct Catalogue {
    pub tasks: Vec<super::source::Task>,
    /// Eligible disk sources, including empty files edited in the current buffer.
    pub sources: std::collections::HashSet<std::path::PathBuf>,
    /// Explicit imports and all resolution alternatives, including absent files.
    /// A watch event on any of them invalidates the owning catalogue.
    pub dependencies: std::collections::HashSet<std::path::PathBuf>,
}

pub fn catalogue(project: &Path) -> Catalogue {
    use super::source::{self, Runner as Kind};
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::io::Read;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut result = Catalogue::default();
    let mut imported = HashSet::new();
    let mut unknown = HashSet::new();
    let mut unresolved = HashSet::new();
    let mut owner_imports = HashMap::<(Kind, PathBuf), HashSet<PathBuf>>::new();
    let mut import_edges = 0;
    let mut cache = HashMap::<PathBuf, String>::new();
    let mut queue = VecDeque::new();
    let mut bytes = 0;
    let canonical_project = match project.canonicalize() {
        Ok(path) => path,
        Err(_) => return result,
    };
    let read = |path: &Path, bytes: &mut usize| -> Option<String> {
        // Bound individual reads and total retained source bytes, including
        // explicit files outside the conventional-name walk.
        let metadata = std::fs::symlink_metadata(path).ok()?;
        if !metadata.is_file() || metadata.len() > 1024 * 1024 {
            return None;
        }
        let mut text = String::new();
        std::fs::File::open(path)
            .ok()?
            .take(1024 * 1024 + 1)
            .read_to_string(&mut text)
            .ok()?;
        *bytes += text.len();
        (text.len() <= 1024 * 1024 && *bytes <= 64 * 1024 * 1024).then_some(text)
    };
    let walk = ignore::WalkBuilder::new(project)
        .parents(false)
        .hidden(false)
        .follow_links(false)
        .require_git(false)
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some(".git" | ".guild" | "node_modules" | "target")
            )
        })
        .build();
    for (index, entry) in walk.enumerate() {
        if index >= 20_000 || Instant::now() >= deadline || bytes > 64 * 1024 * 1024 {
            result.tasks.clear();
            result.sources.clear();
            return result;
        }
        let Ok(entry) = entry else {
            result.tasks.clear();
            result.sources.clear();
            return result;
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(project) else {
            continue;
        };
        let Some(runner) = Kind::of(relative) else {
            continue;
        };
        result.dependencies.insert(relative.to_owned());
        let Some(text) = read(entry.path(), &mut bytes) else {
            unresolved.insert((runner, relative.to_owned()));
            continue;
        };
        result.sources.insert(relative.to_owned());
        let mut found = source::parse(relative, &text);
        for task in &mut found {
            if task.identity.runner == Kind::Package {
                task.managers = source::managers(project, relative, &task.managers);
            }
        }
        result.tasks.extend(found);
        cache.insert(relative.to_owned(), text);
        queue.push_back((
            runner,
            relative.to_owned(),
            relative.parent().unwrap_or(Path::new("")).to_owned(),
            Vec::<PathBuf>::new(),
            relative.to_owned(),
        ));
    }
    let mut visited = HashSet::new();
    while let Some((runner, path, cwd, mut ancestors, owner)) = queue.pop_front() {
        if ancestors.contains(&path) || ancestors.len() >= 256 {
            unresolved.insert((runner, owner.clone()));
            continue;
        }
        if !visited.insert((runner, path.clone(), cwd.clone(), owner.clone())) {
            continue;
        }
        if visited.len() > 20_000 || Instant::now() >= deadline || bytes > 64 * 1024 * 1024 {
            result.tasks.clear();
            result.sources.clear();
            return result;
        }
        if !cache.contains_key(&path) {
            let Some(text) = read(&project.join(&path), &mut bytes) else {
                unresolved.insert((runner, owner.clone()));
                continue;
            };
            cache.insert(path.clone(), text);
        }
        let Some(groups) = source::imports_in_context(runner, &path, &cwd, &cache[&path]) else {
            unresolved.insert((runner, owner.clone()));
            continue;
        };
        ancestors.push(path);
        for candidates in groups {
            let mut resolved = false;
            for child in candidates {
                result.dependencies.insert(child.clone());
                if owner_imports
                    .entry((runner, owner.clone()))
                    .or_default()
                    .insert(child.clone())
                {
                    import_edges += 1;
                }
                if result.dependencies.len() > 20_000
                    || import_edges > 20_000
                    || Instant::now() >= deadline
                {
                    result.tasks.clear();
                    result.sources.clear();
                    return result;
                }
                imported.insert(child.clone());
                let absolute = project.join(&child);
                // Never escape the project through explicit imports. An unknown
                // owner fails closed for its runner.
                match absolute.canonicalize() {
                    Ok(canonical) if canonical.starts_with(&canonical_project) => {
                        if !absolute.is_dir() {
                            resolved = true;
                            queue.push_back((
                                runner,
                                child,
                                cwd.clone(),
                                ancestors.clone(),
                                owner.clone(),
                            ));
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                        ) => {}
                    _ => {
                        unresolved.insert((runner, owner.clone()));
                    }
                }
            }
            if !resolved {
                unresolved.insert((runner, owner.clone()));
            }
        }
    }
    // A conventional file can turn out to be imported after another owner's
    // nonconventional chain is read. Its speculative standalone cwd is not an
    // invocation context and cannot invalidate a successfully resolved owner.
    // A self-importing or mutually cyclic owner is not a verified invocation.
    // Only a successfully traced owner's imports can dismiss another owner's
    // speculative standalone failure; otherwise genuine cycles fail closed.
    let verified_imports: HashSet<_> = owner_imports
        .into_iter()
        .filter(|(owner, _)| !unresolved.contains(owner))
        .flat_map(|(_, imports)| imports)
        .collect();
    unknown.extend(
        unresolved
            .into_iter()
            .filter_map(|(runner, owner)| (!verified_imports.contains(&owner)).then_some(runner)),
    );
    result
        .sources
        .retain(|path| !imported.contains(path) && !unknown.contains(&Kind::of(path).unwrap()));
    result
        .tasks
        .retain(|task| result.sources.contains(&task.identity.source));
    result
}

fn just(project: &Path) -> Vec<String> {
    declared(project)
        .into_iter()
        .filter(|line| line.contains("'just'"))
        .collect()
}

/// The names on `just --summary`'s one line, each as a command.
///
/// The order is `just`'s own, which is alphabetical; sorting it again
/// would only be a second opinion about the same list.
pub fn just_recipes(summary: &str) -> Vec<String> {
    summary
        .split_whitespace()
        .map(|recipe| format!("just {recipe}"))
        .collect()
}

/// The `scripts` of a `package.json` beside the project.
fn npm(project: &Path) -> Vec<String> {
    match std::fs::read_to_string(project.join("package.json")) {
        Ok(text) => npm_scripts(&text),
        Err(err) => {
            tracing::warn!(%err, "package.json could not be read");
            Vec::new()
        }
    }
}

/// The scripts a `package.json` declares, each as `npm run <name>`.
///
/// A file with no `scripts`, a `scripts` that is not an object, and a
/// file that is not JSON at all are each **nothing to offer** rather than
/// a failure: a project is not obliged to be tidy for us, and a notice
/// about somebody else's `package.json` is a notice nobody asked for.
///
/// **The order is ours, and sorted on purpose.** `serde_json`'s map keeps
/// insertion order or sorts, depending on whether anything in the graph
/// turned on its `preserve_order` feature — something in ours has, and
/// nothing of ours asked for it. An order that changes when a dependency
/// three crates away changes a feature flag is not an order; sorting
/// makes the answer ours, and puts these rows in the same order
/// `just --summary` already gives its own.
pub fn npm_scripts(package: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(package) else {
        return Vec::new();
    };
    let Some(scripts) = value.get("scripts").and_then(|scripts| scripts.as_object()) else {
        return Vec::new();
    };
    let mut names: Vec<&String> = scripts.keys().collect();
    names.sort();
    names
        .into_iter()
        .map(|name| format!("npm run {name}"))
        .collect()
}
