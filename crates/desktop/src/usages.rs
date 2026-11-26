//! Find Usages — what can be searched for, and what a server's answer
//! becomes. M8.15.
//!
//! **The grammar is asked before any server**, decision 2 of the plan.
//! Task 0 measured why the server cannot be left to decide: rust-analyzer
//! answers `fn` with the keyword itself as a usage, jdtls answers `public`
//! with the method it modifies, and gopls refuses `func` with *no
//! identifier found*. A caret that is not on a name is told so at once,
//! and no process is asked anything.
//!
//! Pure: a text, a grammar, an offset, an answer.

use std::ops::Range;

use tree_sitter::Node;

/// The name the caret is on — what is searched for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Where the name is, in bytes of the text. The server is asked at
    /// its **start**, not at the caret: a caret just past a word is on
    /// the word for a person and on the next token for a server.
    pub range: Range<usize>,
    /// The name as it is written.
    pub name: String,
}

/// Leaves that are names without saying `identifier` in their kind.
///
/// Measured against the six grammars whose languages have a server:
/// rust, go, java and kotlin call every name some `…identifier`; Ruby's
/// constants and variables are `constant`, `instance_variable`,
/// `class_variable` and `global_variable`, and Elixir's module names are
/// `alias`. Keywords are **unnamed** nodes in all six, which is what
/// keeps `fn`, `func`, `public` and `class` out without a list of them.
const NAMES: &[&str] = &[
    "constant",
    "instance_variable",
    "class_variable",
    "global_variable",
    "alias",
];

/// The name at `offset` in `text`, read by the language's grammar, or
/// `None` when the caret is on a keyword, a literal, a comment, a
/// space — anything a person cannot find the usages of.
///
/// A caret just past a word counts as on it: that is
/// where a double-click-and-arrow or a typed name leaves it.
pub fn target_at(language: &str, text: &str, offset: usize) -> Option<Target> {
    let grammar = crate::grammars::of(language)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&(grammar.language)()).ok()?;
    let tree = parser.parse(text, None)?;
    let root = tree.root_node();

    let offset = offset.min(text.len());
    let name = name_at(root, offset).or_else(|| {
        offset
            .checked_sub(1)
            .and_then(|before| name_at(root, before))
    })?;
    Some(Target {
        range: name.byte_range(),
        name: text.get(name.byte_range())?.to_owned(),
    })
}

/// The name whose bytes include `offset`, if the leaf there is one.
fn name_at(root: Node<'_>, offset: usize) -> Option<Node<'_>> {
    let node = root.named_descendant_for_byte_range(offset, offset)?;
    let covers = node.start_byte() <= offset && offset < node.end_byte();
    (covers && node.child_count() == 0 && is_name(node.kind())).then_some(node)
}

/// Whether a leaf of this kind is a name.
fn is_name(kind: &str) -> bool {
    kind.ends_with("identifier") || NAMES.contains(&kind)
}

/// Which of the two lists a usage is in.
///
/// The first is *Usages in Project Files*, and a usage anywhere else — a
/// `jar:` source, a toolchain's library — is under a group of its own,
/// because it opens a file the person did not write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    /// Inside the project's own root.
    Project,
    /// Anywhere else.
    Libraries,
}

/// What a server's answer came to.
///
/// **Two parallel lists**, a row each and a link each, because the rows
/// are what `search_tree` and the painting already take, and the link is
/// what `land_on` already opens — a jar, a dependency or a project file,
/// three routes that are not worth a second copy.
#[derive(Debug, Clone, Default)]
pub struct Found {
    /// What each usage draws: its line, cut, with the name placed in it.
    /// The project's first, then the libraries'.
    pub hits: Vec<crate::search::Hit>,
    /// Where each usage is, as the server said it.
    pub links: Vec<lsp_types::LocationLink>,
    /// How many of them are the project's: `hits[..project]`.
    pub project: usize,
}

impl Found {
    /// The usages of one group, and the index its first one has.
    pub fn of(&self, group: Group) -> (&[crate::search::Hit], usize) {
        match group {
            Group::Project => (&self.hits[..self.project], 0),
            Group::Libraries => (&self.hits[self.project..], self.project),
        }
    }

    /// How many usages there are.
    pub fn len(&self) -> usize {
        self.hits.len()
    }

    /// Whether the server found nothing.
    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }
}

/// The rows of a server's answer.
///
/// `text_of` reads a whole file by its URI — the open buffer, the disk or
/// a jar — and is asked **once per file**, however many usages it holds.
/// A file that cannot be read keeps its usages, drawn as their place
/// alone: a usage the server found is not hidden because its line could
/// not be shown.
///
/// Sorted by path and place, and a place the server named twice is one
/// usage.
pub fn found(
    links: Vec<lsp_types::LocationLink>,
    root: &std::path::Path,
    mut text_of: impl FnMut(&str) -> Option<String>,
) -> Found {
    let mut texts: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    let mut project = Vec::new();
    let mut libraries = Vec::new();
    for link in links {
        let uri = link.target_uri.to_string();
        let (group, path) = place_of(&uri, root);
        let text = texts.entry(uri.clone()).or_insert_with(|| text_of(&uri));
        let hit = hit_at(path, text.as_deref(), link.target_selection_range);
        match group {
            Group::Project => project.push((hit, link)),
            Group::Libraries => libraries.push((hit, link)),
        }
    }
    let order = |(hit, link): &(crate::search::Hit, lsp_types::LocationLink)| {
        (
            hit.path.clone(),
            hit.line,
            link.target_selection_range.start.character,
        )
    };
    for list in [&mut project, &mut libraries] {
        list.sort_by_key(order);
        list.dedup_by_key(|pair| order(pair));
    }
    let count = project.len();
    let (hits, links) = project.into_iter().chain(libraries).unzip();
    Found {
        hits,
        links,
        project: count,
    }
}

/// Which group a URI is in, and the path its row is filed under.
///
/// A project file is filed by its path in the project, as every tree in
/// the window files it. A `jar:` entry is filed under the jar's name and
/// then its path inside — `feign-core-13.6-sources.jar/feign/Contract.java`
/// — and any other file under its whole path with the home directory as
/// `~`, which the tree then collapses to one row.
fn place_of(uri: &str, root: &std::path::Path) -> (Group, std::path::PathBuf) {
    if let Some(path) = crate::lsp::path_of(uri) {
        if let Ok(relative) = path.strip_prefix(root) {
            return (Group::Project, relative.to_path_buf());
        }
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let shown = match home.as_deref().map(|home| path.strip_prefix(home)) {
            Some(Ok(inside)) => std::path::Path::new("~").join(inside),
            _ => path,
        };
        return (Group::Libraries, shown);
    }
    let shown = match crate::jars::parse(uri) {
        Some(entry) => {
            let jar = entry
                .jar
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            std::path::Path::new(&jar).join(entry.inside.trim_start_matches('/'))
        }
        None => std::path::PathBuf::from(uri),
    };
    (Group::Libraries, shown)
}

/// One usage as a row: its line, cut, with the name placed in it.
///
/// The protocol's range counts UTF-16 units; the row's counts bytes, and
/// a range that runs past its line is cut at the line's end.
fn hit_at(
    path: std::path::PathBuf,
    text: Option<&str>,
    range: lsp_types::Range,
) -> crate::search::Hit {
    let line = text
        .and_then(|text| text.lines().nth(range.start.line as usize))
        .unwrap_or("");
    let start = byte_of(line, range.start.character);
    let end = if range.end.line == range.start.line {
        byte_of(line, range.end.character).max(start)
    } else {
        line.len()
    };
    let column = line[..start].chars().count() as u32;
    let ranges: Vec<Range<usize>> = (end > start).then_some(start..end).into_iter().collect();
    let (text, ranges, cut) = crate::search::cut_around(line, ranges);
    crate::search::Hit {
        path,
        line: range.start.line + 1,
        text,
        ranges,
        cut,
        column,
    }
}

/// The byte in `line` that `units` UTF-16 units reach, clamped to its end.
fn byte_of(line: &str, units: u32) -> usize {
    let mut counted = 0;
    for (at, c) in line.char_indices() {
        if counted >= units {
            return at;
        }
        counted += c.len_utf16() as u32;
    }
    line.len()
}

/// One line of a Find tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// What was searched for — the first row, *Contract of feign*.
    Target,
    /// *Usages in Project Files · 3 results*, or the libraries' line.
    Group {
        group: Group,
        /// How many usages are under it.
        count: usize,
    },
    /// A directory, a file or a usage of one group, as `search_tree`
    /// builds them. A usage's `at` indexes [`Found::hits`], not the
    /// group's own list, and the depth is one deeper than it says,
    /// being under a group.
    Item {
        group: Group,
        row: crate::search_tree::Row,
    },
}

/// What is folded in a Find tab.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folds {
    /// Groups shut whole.
    pub groups: std::collections::HashSet<Group>,
    /// Directories and files shut within a group, by the path the tree
    /// files them under.
    pub paths: std::collections::HashSet<(Group, std::path::PathBuf)>,
}

impl Folds {
    /// Open what is shut, shut what is open. A usage and the target do
    /// not fold, and answer `false`.
    pub fn toggle(&mut self, row: &Row) -> bool {
        match row {
            Row::Target => false,
            Row::Group { group, .. } => {
                if !self.groups.remove(group) {
                    self.groups.insert(*group);
                }
                true
            }
            Row::Item { group, row } => match crate::search_tree::foldable(row) {
                Some(path) => {
                    let key = (*group, path);
                    if !self.paths.remove(&key) {
                        self.paths.insert(key);
                    }
                    true
                }
                None => false,
            },
        }
    }

    /// Whether a row is shut.
    pub fn is_folded(&self, row: &Row) -> bool {
        match row {
            Row::Target => false,
            Row::Group { group, .. } => self.groups.contains(group),
            Row::Item { group, row } => crate::search_tree::foldable(row)
                .is_some_and(|path| self.paths.contains(&(*group, path))),
        }
    }

    /// Shut every group, directory and file there is.
    pub fn fold_all(&mut self, found: &Found) {
        *self = Self::default();
        for row in rows(found, &Self::default()) {
            self.toggle(&row);
        }
    }
}

/// The rows of a Find tab, in the order they are drawn: the target, then
/// each group that has usages, and under it the group's tree.
pub fn rows(found: &Found, folds: &Folds) -> Vec<Row> {
    let mut rows = vec![Row::Target];
    for group in [Group::Project, Group::Libraries] {
        let (hits, first) = found.of(group);
        if hits.is_empty() {
            continue;
        }
        rows.push(Row::Group {
            group,
            count: hits.len(),
        });
        if folds.groups.contains(&group) {
            continue;
        }
        let shut = folds
            .paths
            .iter()
            .filter(|(of, _)| *of == group)
            .map(|(_, path)| path.clone())
            .collect();
        rows.extend(
            crate::search_tree::of(hits, &shut)
                .into_iter()
                .map(|row| Row::Item {
                    group,
                    row: match row {
                        crate::search_tree::Row::Hit { at, depth } => {
                            crate::search_tree::Row::Hit {
                                at: at + first,
                                depth,
                            }
                        }
                        other => other,
                    },
                }),
        );
    }
    rows
}

/// The usage a row is, if it is one — an index into [`Found::hits`].
pub fn usage_of(row: &Row) -> Option<usize> {
    match row {
        Row::Item {
            row: crate::search_tree::Row::Hit { at, .. },
            ..
        } => Some(*at),
        _ => None,
    }
}

/// File identity keeps project and external sources separate; match rows remain locations.
pub fn file_of(row: &Row) -> Option<(Group, std::path::PathBuf)> {
    match row {
        Row::Item {
            group,
            row: crate::search_tree::Row::File { path, .. },
        } => Some((*group, path.clone())),
        _ => None,
    }
}
