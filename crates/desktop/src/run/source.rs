//! Source declarations, shared by the editor and the command chooser.
use std::path::{Path, PathBuf};
use tree_sitter::Node;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Runner {
    Just,
    Make,
    Package,
    Task,
}
impl Runner {
    pub fn of(path: &Path) -> Option<Self> {
        match path.file_name()?.to_str()?.to_ascii_lowercase().as_str() {
            "justfile" | ".justfile" => Some(Self::Just),
            "makefile" | "gnumakefile" => Some(Self::Make),
            "package.json" => Some(Self::Package),
            "taskfile.yml" | "taskfile.yaml" | "taskfile.dist.yml" | "taskfile.dist.yaml" => {
                Some(Self::Task)
            }
            _ => None,
        }
    }
    pub fn program(self) -> &'static str {
        match self {
            Self::Just => "just",
            Self::Make => "make",
            Self::Package => "npm",
            Self::Task => "task",
        }
    }
    fn grammar(self) -> &'static str {
        match self {
            Self::Just => "just",
            Self::Make => "make",
            Self::Package => "json",
            Self::Task => "yaml",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identity {
    pub source: PathBuf,
    pub runner: Runner,
    pub name: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub required: bool,
    pub variadic: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub identity: Identity,
    pub offset: usize,
    pub row: usize,
    pub parameters: Vec<Parameter>,
    pub managers: Vec<String>,
    pub default: bool,
}
impl Task {
    pub fn chooser_label(&self) -> String {
        format!("{}: {}", self.identity.source.display(), self.identity.name)
    }
    pub fn cwd(&self, project: &Path) -> PathBuf {
        project.join(self.identity.source.parent().unwrap_or(Path::new("")))
    }
    pub fn argv(&self, values: &[String], manager: Option<&str>) -> Result<Vec<String>, String> {
        if self.parameters.iter().filter(|p| p.required).count() > values.len() {
            return Err("Required task arguments are missing".into());
        }
        let file = self
            .identity
            .source
            .file_name()
            .ok_or("Task source has no filename")?
            .to_string_lossy()
            .into_owned();
        let name = &self.identity.name;
        if name.starts_with('-') {
            return Err("Task name would be interpreted as a runner option".into());
        }
        let mut argv = match self.identity.runner {
            Runner::Just => vec!["just".into(), "--justfile".into(), file, "--".into()],
            Runner::Make => vec!["make".into(), "--file".into(), file],
            Runner::Task => vec!["task".into(), "--taskfile".into(), file],
            Runner::Package => {
                let manager = manager
                    .or_else(|| (self.managers.len() == 1).then(|| self.managers[0].as_str()))
                    .ok_or("Choose a package manager")?;
                if !["npm", "pnpm", "yarn", "bun"].contains(&manager) {
                    return Err("Unsupported package manager".into());
                }
                vec![manager.into(), "run".into()]
            }
        };
        if !name.is_empty() {
            argv.push(name.clone());
        }
        if matches!(self.identity.runner, Runner::Make | Runner::Task)
            && values.iter().any(|v| !valid_variable(v))
        {
            return Err("Variables must use NAME=value".into());
        }
        argv.extend(values.iter().cloned());
        Ok(argv)
    }
    pub fn command(&self, values: &[String], manager: Option<&str>) -> Result<String, String> {
        self.argv(values, manager).map(|argv| shell_line(&argv))
    }
}
fn valid_variable(value: &str) -> bool {
    value.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !name.as_bytes()[0].is_ascii_digit()
    })
}
/// Every generated argument is data, including the executable and task name.
pub fn shell_line(argv: &[String]) -> String {
    argv.iter()
        .map(|token| format!("'{}'", token.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}
fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}
/// Split a parameter field as data; substitutions and operators are never evaluated.
pub fn tokens(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut escape = false;
    let mut begun = false;
    for ch in input.chars() {
        if escape {
            token.push(ch);
            escape = false;
            begun = true;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escape = true;
            begun = true;
            continue;
        }
        if Some(ch) == quote {
            quote = None;
            continue;
        }
        if quote.is_none() && matches!(ch, '\'' | '"') {
            quote = Some(ch);
            begun = true;
            continue;
        }
        if quote.is_none() && ch.is_whitespace() {
            if begun {
                tokens.push(std::mem::take(&mut token));
                begun = false;
            }
        } else {
            token.push(ch);
            begun = true;
        }
    }
    if quote.is_some() || escape {
        return Err("Unclosed argument quote or escape".into());
    }
    if begun {
        tokens.push(token);
    }
    Ok(tokens)
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}
fn descendant<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    children(node)
        .into_iter()
        .find_map(|child| descendant(child, kind))
}
fn scalar(node: Node<'_>, source: &str) -> Option<String> {
    if descendant(node, "block_mapping").is_some() || descendant(node, "block_sequence").is_some() {
        return None;
    }
    let raw = text(node, source).trim();
    if raw.starts_with('"') {
        serde_json::from_str(raw).ok()
    } else if raw.starts_with('\'') && raw.ends_with('\'') {
        Some(raw[1..raw.len() - 1].replace("''", "'"))
    } else if !raw.is_empty() && !raw.contains(['\n', '&', '*', '{', '}']) {
        Some(raw.to_owned())
    } else {
        None
    }
}

/// Parse a bounded source snapshot; no runner or template is ever evaluated.
pub fn parse(path: &Path, source: &str) -> Vec<Task> {
    let Some(runner) = Runner::of(path) else {
        return vec![];
    };
    if source.len() > 1024 * 1024 {
        return vec![];
    }
    let mut parser = tree_sitter::Parser::new();
    let grammar = crate::grammars::of(runner.grammar()).expect("registered task grammar");
    if parser.set_language(&(grammar.language)()).is_err() {
        return vec![];
    }
    let Some(tree) = parser.parse(source, None) else {
        return vec![];
    };
    let root = tree.root_node();
    if root.has_error() {
        return vec![];
    }
    let make_task = |name: String, node: Node<'_>, parameters: Vec<Parameter>| Task {
        identity: Identity {
            source: path.to_owned(),
            runner,
            name,
        },
        offset: node.start_byte(),
        row: node.start_position().row,
        parameters,
        managers: vec!["npm".into()],
        default: false,
    };
    let mut tasks = Vec::new();
    match runner {
        Runner::Just => {
            for node in children(root).into_iter().filter(|n| n.kind() == "recipe") {
                let Some(name) = node.child_by_field_name("name") else {
                    continue;
                };
                // Private and platform-conditional recipes are not unconditional actions.
                let attributes: Vec<_> = children(node)
                    .into_iter()
                    .filter(|n| n.kind() == "attributes")
                    .flat_map(children)
                    .filter_map(|n| n.child_by_field_name("name"))
                    .map(|n| text(n, source))
                    .collect();
                if attributes
                    .iter()
                    .any(|a| matches!(*a, "private" | "linux" | "windows" | "macos" | "unix"))
                {
                    continue;
                }
                let parameters = children(node)
                    .into_iter()
                    .filter(|n| matches!(n.kind(), "parameter" | "variadic_parameter"))
                    .filter_map(|n| {
                        let variadic = n.kind() == "variadic_parameter";
                        let p = if variadic {
                            descendant(n, "parameter")?
                        } else {
                            n
                        };
                        Some(Parameter {
                            name: text(p.child_by_field_name("name")?, source).into(),
                            required: p.child_by_field_name("default").is_none()
                                && (!variadic || text(n, source).starts_with('+')),
                            variadic,
                        })
                    })
                    .collect();
                let mut task = make_task(text(name, source).into(), name, parameters);
                task.default = attributes.contains(&"default");
                tasks.push(task);
            }
        }
        Runner::Make => {
            for rule in children(root).into_iter().filter(|n| n.kind() == "rule") {
                let Some(targets) = children(rule).into_iter().find(|n| n.kind() == "targets")
                else {
                    continue;
                };
                for target in children(targets) {
                    let name = text(target, source);
                    if target.kind() == "word"
                        && !name.starts_with(['.', '-'])
                        && !name.contains(['%', '$', ':', '(', ')', '*', '?', '\\'])
                    {
                        tasks.push(make_task(name.into(), target, vec![]));
                    }
                }
            }
        }
        Runner::Package => {
            let Ok(package) = serde_json::from_str::<serde_json::Value>(source) else {
                return vec![];
            };
            let Some(scripts) = package.get("scripts").and_then(|v| v.as_object()) else {
                return vec![];
            };
            let object = children(root).into_iter().find(|n| n.kind() == "object");
            let pairs = object.map(children).unwrap_or_default();
            let script_pair = pairs.into_iter().find(|n| {
                n.child_by_field_name("key").is_some_and(|k| {
                    serde_json::from_str::<String>(text(k, source))
                        .ok()
                        .as_deref()
                        == Some("scripts")
                })
            });
            if let Some(object) = script_pair.and_then(|n| n.child_by_field_name("value")) {
                for pair in children(object) {
                    let Some(key) = pair.child_by_field_name("key") else {
                        continue;
                    };
                    let Ok(name) = serde_json::from_str::<String>(text(key, source)) else {
                        continue;
                    };
                    if scripts.get(&name).is_some_and(|v| v.is_string()) {
                        let mut task = make_task(name, key, vec![]);
                        task.managers = package
                            .get("packageManager")
                            .and_then(|v| v.as_str())
                            .map(|s| s.split('@').next().unwrap_or(""))
                            .filter(|m| ["npm", "pnpm", "yarn", "bun"].contains(m))
                            .map(|m| vec![m.to_owned()])
                            .unwrap_or_default();
                        tasks.push(task);
                    }
                }
            }
        }
        Runner::Task => {
            let Some(mapping) = descendant(root, "block_mapping") else {
                return vec![];
            };
            let pairs = children(mapping);
            if !pairs.iter().any(|p| {
                p.child_by_field_name("key")
                    .and_then(|k| scalar(k, source))
                    .as_deref()
                    == Some("version")
            }) {
                return vec![];
            }
            let task_pair = pairs.iter().find(|p| {
                p.child_by_field_name("key")
                    .and_then(|k| scalar(k, source))
                    .as_deref()
                    == Some("tasks")
            });
            if let Some(mapping) = task_pair
                .and_then(|p| p.child_by_field_name("value"))
                .and_then(|v| descendant(v, "block_mapping"))
            {
                for pair in children(mapping) {
                    let Some(key) = pair.child_by_field_name("key") else {
                        continue;
                    };
                    let Some(name) = scalar(key, source) else {
                        continue;
                    };
                    let Some(value) = pair.child_by_field_name("value") else {
                        continue;
                    };
                    let internal = descendant(value, "block_mapping").is_some_and(|mapping| {
                        children(mapping).into_iter().any(|pair| {
                            pair.child_by_field_name("key")
                                .and_then(|key| scalar(key, source))
                                .as_deref()
                                == Some("internal")
                                && pair
                                    .child_by_field_name("value")
                                    .and_then(|value| scalar(value, source))
                                    .as_deref()
                                    == Some("true")
                        })
                    });
                    if !name.contains(['{', '*', '?']) && !name.starts_with('-') && !internal {
                        tasks.push(make_task(name, key, vec![]));
                    }
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    tasks.retain(|task| seen.insert(task.identity.clone()));
    tasks
}

/// Manager evidence at the nearest directory wins, explicit packageManager first.
pub fn managers(project: &Path, source: &Path, explicit: &[String]) -> Vec<String> {
    use std::io::Read;
    if !explicit.is_empty() {
        return explicit.to_vec();
    }
    let mut directory = project.join(source.parent().unwrap_or(Path::new("")));
    loop {
        let mut found = Vec::new();
        let mut bytes = Vec::new();
        if std::fs::File::open(directory.join("package.json"))
            .and_then(|file| file.take(1024 * 1024 + 1).read_to_end(&mut bytes))
            .is_ok()
            && bytes.len() <= 1024 * 1024
            && let Ok(package) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && let Some(manager) = package
                .get("packageManager")
                .and_then(|v| v.as_str())
                .and_then(|v| v.split('@').next())
            && ["npm", "pnpm", "yarn", "bun"].contains(&manager)
        {
            return vec![manager.to_owned()];
        }
        for (name, manager) in [
            ("package-lock.json", "npm"),
            ("npm-shrinkwrap.json", "npm"),
            ("pnpm-lock.yaml", "pnpm"),
            ("yarn.lock", "yarn"),
            ("bun.lock", "bun"),
            ("bun.lockb", "bun"),
        ] {
            if directory.join(name).is_file() && !found.iter().any(|m| m == manager) {
                found.push(manager.to_owned());
            }
        }
        if !found.is_empty() {
            return found;
        }
        if directory == project || !directory.pop() || !directory.starts_with(project) {
            return vec!["npm".into()];
        }
    }
}

/// Referenced declarations cannot be offered as standalone task files.
pub fn imports(path: &Path, source: &str) -> Option<Vec<PathBuf>> {
    let Some(runner) = Runner::of(path) else {
        return Some(vec![]);
    };
    Some(
        imports_in_context(runner, path, path.parent().unwrap_or(Path::new("")), source)?
            .into_iter()
            .flatten()
            .collect(),
    )
}

/// An explicit imported file uses its owner's grammar even when its name isn't
/// conventional. Make includes retain the invocation cwd throughout the chain.
pub(crate) fn imports_in_context(
    runner: Runner,
    path: &Path,
    cwd: &Path,
    source: &str,
) -> Option<Vec<Vec<PathBuf>>> {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&(crate::grammars::of(runner.grammar()).unwrap().language)())
        .is_err()
    {
        return None;
    }
    let tree = parser.parse(source, None)?;
    if tree.root_node().has_error() {
        return None;
    }
    let parent = if runner == Runner::Make {
        cwd
    } else {
        path.parent().unwrap_or(Path::new(""))
    };
    let mut imports = Vec::new();
    let mut nodes = children(tree.root_node());
    // Includes nested in Make conditionals still establish an owning context.
    // Inspect both branches without evaluating their condition.
    if runner == Runner::Make {
        let mut index = 0;
        while index < nodes.len() {
            nodes.extend(children(nodes[index]));
            index += 1;
        }
    }
    for node in nodes {
        if matches!(node.kind(), "import" | "module") {
            if let Some(p) = node.child_by_field_name("path") {
                let p = scalar(p, source)?;
                if p.contains(['{', '$', '*']) {
                    return None;
                }
                imports.push(vec![parent.join(p)]);
            } else if node.kind() == "module" {
                let name = text(node.child_by_field_name("name")?, source);
                imports.push(
                    [
                        format!("{name}.just"),
                        format!("{name}/mod.just"),
                        format!("{name}/justfile"),
                        format!("{name}/.justfile"),
                        format!("{name}/Justfile"),
                    ]
                    .map(|p| parent.join(p))
                    .to_vec(),
                );
            } else {
                return None;
            }
        }
        if node.kind() == "include_directive" {
            let filenames = node.child_by_field_name("filenames")?;
            for filename in children(filenames) {
                let p = text(filename, source);
                if filename.kind() != "word" || p.contains(['$', '*', '?', '{', '\\']) {
                    return None;
                }
                imports.push(vec![parent.join(p)]);
            }
        }
    }
    if runner == Runner::Task
        && let Some(mapping) = descendant(tree.root_node(), "block_mapping")
    {
        for pair in children(mapping) {
            if pair
                .child_by_field_name("key")
                .and_then(|k| scalar(k, source))
                .as_deref()
                != Some("includes")
            {
                continue;
            }
            if let Some(mapping) = pair
                .child_by_field_name("value")
                .and_then(|v| descendant(v, "block_mapping"))
            {
                for include in children(mapping) {
                    if let Some(value) = include.child_by_field_name("value") {
                        let p = scalar(value, source).or_else(|| {
                            children(descendant(value, "block_mapping")?)
                                .into_iter()
                                .find_map(|pair| {
                                    (pair
                                        .child_by_field_name("key")
                                        .and_then(|k| scalar(k, source))
                                        .as_deref()
                                        == Some("taskfile"))
                                    .then(|| {
                                        pair.child_by_field_name("value")
                                            .and_then(|v| scalar(v, source))
                                    })
                                    .flatten()
                                })
                        });
                        let p = p?;
                        if p.contains(['{', '$', '*']) {
                            return None;
                        }
                        let mut candidates = vec![parent.join(&p)];
                        for name in [
                            "Taskfile.yml",
                            "Taskfile.yaml",
                            "taskfile.yml",
                            "taskfile.yaml",
                        ] {
                            candidates.push(parent.join(&p).join(name));
                        }
                        imports.push(candidates);
                    }
                }
            }
        }
    }
    imports
        .into_iter()
        .map(|paths| {
            paths
                .into_iter()
                .map(|path| {
                    let mut clean = PathBuf::new();
                    for component in path.components() {
                        match component {
                            std::path::Component::CurDir => {}
                            std::path::Component::ParentDir => {
                                if !clean.pop() {
                                    return None;
                                }
                            }
                            std::path::Component::Normal(part) => clean.push(part),
                            _ => return None,
                        }
                    }
                    Some(clean)
                })
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()
}
