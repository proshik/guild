//! Which hosts glab knows and is signed in to, read from its own config
//! file without the network — GLD-063.
//!
//! `glab auth status` asks GitLab (spec review r2, finding 2); this does
//! not. glab 1.118 keeps a `hosts:` map in `config.yml`; a host with a
//! non-empty `token`, or with `use_keyring: true` (the token is in the
//! system keyring), is signed in. A host listed with neither is known and
//! signed out. **No token is kept**: a token's value is read only as far as
//! "not empty" and dropped with the line.
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// What glab says about a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Login {
    /// Signed in, under this glab host name (the one `--hostname` takes).
    SignedIn(String),
    /// Configured, with no credential.
    SignedOut,
    /// glab does not know the host.
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Entry {
    ssh_host: Option<String>,
    signed_in: bool,
    proxy: Option<String>,
    subfolder: Option<String>,
}

/// glab's hosts, as its config file lists them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hosts {
    entries: BTreeMap<String, Entry>,
}

impl Hosts {
    /// The config file glab 1.118 reads, as its `SearchConfigFile`
    /// (`internal/config/config_file.go`) decides (code review r1,
    /// arch+quality-4):
    ///
    /// 1. `GLAB_CONFIG_DIR`, when set, and nothing else — even with no
    ///    `config.yml` in it, glab does not fall through;
    /// 2. the legacy `~/.config/glab-cli/config.yml`, when it exists;
    /// 3. XDG's config home: `$XDG_CONFIG_HOME/glab-cli`, else on macOS
    ///    `~/Library/Application Support/glab-cli` (adrg/xdg's default
    ///    there; not read from its source, observed as the person's).
    ///
    /// `None` when none of 2 and 3 holds a file. XDG's system directories,
    /// which glab searches after the user's, are not looked at.
    pub fn config_file(
        home: &Path,
        glab_config_dir: Option<&OsStr>,
        xdg_config_home: Option<&OsStr>,
    ) -> Option<PathBuf> {
        if let Some(dir) = glab_config_dir.filter(|dir| !dir.is_empty()) {
            return Some(PathBuf::from(dir).join("config.yml"));
        }
        let legacy = home.join(".config/glab-cli/config.yml");
        if legacy.is_file() {
            return Some(legacy);
        }
        let config_home = match xdg_config_home.filter(|dir| !dir.is_empty()) {
            Some(dir) => PathBuf::from(dir),
            None => home.join("Library/Application Support"),
        };
        let file = config_home.join("glab-cli/config.yml");
        file.is_file().then_some(file)
    }

    /// The process's own: `HOME`, `GLAB_CONFIG_DIR` and `XDG_CONFIG_HOME`
    /// as this process sees them.
    pub fn of_this_process() -> Self {
        let Some(home) = std::env::var_os("HOME") else {
            return Self::default();
        };
        // The same absolute folder every glab child is given (code review
        // r2, arch+quality-2).
        let config_dir = super::glab::config_dir_of_this_process();
        match Self::config_file(
            Path::new(&home),
            config_dir.as_deref().map(Path::as_os_str),
            std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        ) {
            Some(file) => Self::read(&file),
            None => Self::default(),
        }
    }

    /// The hosts in a config file; none when it cannot be read.
    pub fn read(file: &Path) -> Self {
        match std::fs::read_to_string(file) {
            Ok(text) => Self::parse(&text),
            Err(_) => Self::default(),
        }
    }

    /// The `hosts:` map of glab's YAML: a host per key one level in, its
    /// settings one level further. Only the four keys this needs are read,
    /// each value as a YAML scalar — quotes, an inline comment, a null and a
    /// flow collection read as YAML reads them (code review r1,
    /// bugs+impl-3).
    pub fn parse(text: &str) -> Self {
        let lines: Vec<&str> = text.lines().collect();
        let mut entries = BTreeMap::new();
        let mut in_hosts = false;
        let mut host_indent: Option<usize> = None;
        let mut current: Option<String> = None;
        let mut at = 0;
        while at < lines.len() {
            let content = lines[at].trim_end();
            at += 1;
            let trimmed = content.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let indent = content.len() - trimmed.len();
            let Some((key, value)) = split_key(trimmed) else {
                continue;
            };
            // A block scalar's value is the lines indented under its key;
            // with none, it is empty (code review r2, bugs+impl-3).
            // A node's properties — a `!tag`, an `&anchor` — come before its
            // block header (code review r3, lenses-8).
            let value = without_properties(value);
            let block;
            let value = if is_block_indicator(value) {
                let start = at;
                while at < lines.len() {
                    let line = lines[at].trim_end();
                    let more = line.trim_start();
                    if !more.is_empty() && line.len() - more.len() <= indent {
                        break;
                    }
                    at += 1;
                }
                block = lines[start..at]
                    .iter()
                    .map(|line| line.trim())
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                Value::Block(&block)
            } else {
                Value::Plain(value)
            };
            if indent == 0 {
                in_hosts = key == "hosts";
                host_indent = None;
                current = None;
                continue;
            }
            if !in_hosts {
                continue;
            }
            match host_indent {
                Some(level) if indent > level => {
                    let Some(entry) = current.as_ref().and_then(|host| entries.get_mut(host))
                    else {
                        continue;
                    };
                    let Entry {
                        ssh_host,
                        signed_in,
                        proxy,
                        subfolder,
                    } = entry;
                    let value = value.read();
                    match key.as_str() {
                        "token" => *signed_in |= value.is_some(),
                        "use_keyring" => {
                            *signed_in |= value.is_some_and(|v| v.eq_ignore_ascii_case("true"))
                        }
                        "ssh_host" => *ssh_host = value.map(|v| v.to_ascii_lowercase()),
                        "proxy" => *proxy = value,
                        "subfolder" => {
                            *subfolder = value
                                .map(|v| v.trim_matches('/').to_owned())
                                .filter(|v| !v.is_empty())
                        }
                        _ => {}
                    }
                }
                _ => {
                    host_indent = Some(indent);
                    let host = key.to_ascii_lowercase();
                    entries.insert(host.clone(), Entry::default());
                    current = Some(host);
                }
            }
        }
        Self { entries }
    }

    /// What glab says about the host a remote names — by the glab host's
    /// own name (with its port, as glab keys it), or, for an ssh or
    /// scp-style remote only, by its `ssh_host` (code review r2,
    /// arch+quality-3).
    pub fn login(&self, remote_host: &str, via_ssh: bool) -> Login {
        match self.entry(remote_host, via_ssh) {
            Some((name, entry)) if entry.signed_in => Login::SignedIn(name.clone()),
            Some(_) => Login::SignedOut,
            None => Login::Unknown,
        }
    }

    /// The proxy glab keeps for a host itself — which glab uses instead of
    /// the environment's (code review r1, arch+quality-2).
    pub fn proxy(&self, glab_host: &str) -> Option<String> {
        self.entry(glab_host, false)
            .and_then(|(_, entry)| entry.proxy.clone())
    }

    /// A remote's project path as glab's API names it on `glab_host`: the
    /// host's `subfolder` is in glab's API base already, so it is cut from
    /// the front of the path at a segment boundary (code review r2,
    /// adversarial-4).
    pub fn project(&self, glab_host: &str, path: &str) -> String {
        let subfolder = self
            .entry(glab_host, false)
            .and_then(|(_, entry)| entry.subfolder.as_deref());
        match subfolder.and_then(|prefix| path.strip_prefix(prefix)?.strip_prefix('/')) {
            Some(rest) if !rest.is_empty() => rest.to_owned(),
            _ => path.to_owned(),
        }
    }

    fn entry(&self, remote_host: &str, via_ssh: bool) -> Option<(&String, &Entry)> {
        let remote_host = remote_host.to_ascii_lowercase();
        self.entries.get_key_value(&remote_host).or_else(|| {
            via_ssh
                .then(|| {
                    self.entries
                        .iter()
                        .find(|(_, entry)| entry.ssh_host.as_deref() == Some(remote_host.as_str()))
                })
                .flatten()
        })
    }
}

/// A mapping line's key and the rest: `key: value`, `key:` alone, a quoted
/// key, and a host key with a port (`example.com:8443:`).
fn split_key(line: &str) -> Option<(String, &str)> {
    if let Some(quote) = line.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let end = line[1..].find(quote)? + 1;
        let rest = line[end + 1..].trim_start().strip_prefix(':')?;
        return Some((line[1..end].to_owned(), rest));
    }
    if let Some((key, value)) = line.split_once(": ") {
        return Some((key.trim().to_owned(), value));
    }
    // A key alone on its line keeps its own colons (`host:8443:`), and an
    // inline comment after it is dropped.
    let line = match line.find(" #") {
        Some(at) => line[..at].trim_end(),
        None => line,
    };
    line.strip_suffix(':')
        .map(|key| (key.trim().to_owned(), ""))
}

/// A mapping value: on its line, or a block scalar's joined lines.
enum Value<'a> {
    Plain(&'a str),
    Block(&'a str),
}

impl Value<'_> {
    fn read(&self) -> Option<String> {
        match self {
            Self::Plain(raw) => scalar(raw),
            Self::Block(text) => (!text.is_empty()).then(|| (*text).to_owned()),
        }
    }
}

/// A value without the node properties before it: each `!tag` and
/// `&anchor`, and the spaces after them.
fn without_properties(raw: &str) -> &str {
    let mut rest = raw.trim_start();
    while rest.starts_with('!') || rest.starts_with('&') {
        rest = rest
            .split_once(char::is_whitespace)
            .map_or("", |(_, after)| after)
            .trim_start();
    }
    rest
}

/// `|` or `>`, with a chomping (`+`, `-`) and an indentation digit in
/// either order, and nothing after it but a comment.
fn is_block_indicator(raw: &str) -> bool {
    let raw = raw.trim();
    let raw = match raw.find(" #") {
        Some(at) => raw[..at].trim_end(),
        None => raw,
    };
    let mut chars = raw.chars();
    matches!(chars.next(), Some('|' | '>'))
        && chars.clone().count() <= 2
        && chars.all(|c| c == '+' || c == '-' || c.is_ascii_digit())
}

/// A YAML scalar that says something: its string, or `None` for an empty
/// value, a null (`~`, `null`), a flow collection (`[]`, `{}`) and an
/// empty quoted string. An inline comment — ` #` outside quotes — is not
/// the value, and a `!!tag` before it is dropped.
fn scalar(raw: &str) -> Option<String> {
    let mut raw = raw.trim();
    if let Some(rest) = raw.strip_prefix('!') {
        raw = rest.split_once(' ').map_or("", |(_, value)| value).trim();
    }
    let value = if let Some(rest) = raw.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        loop {
            match chars.next()? {
                '"' => break,
                '\\' => match chars.next()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    other => out.push(other),
                },
                other => out.push(other),
            }
        }
        out
    } else if let Some(rest) = raw.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = rest.chars().peekable();
        loop {
            match chars.next()? {
                '\'' if chars.peek() == Some(&'\'') => {
                    chars.next();
                    out.push('\'');
                }
                '\'' => break,
                other => out.push(other),
            }
        }
        out
    } else {
        let plain = match raw.find(" #") {
            Some(at) => &raw[..at],
            None if raw.starts_with('#') => "",
            None => raw,
        }
        .trim();
        if plain.starts_with('[') || plain.starts_with('{') {
            return None;
        }
        if matches!(plain, "~" | "null" | "Null" | "NULL") {
            return None;
        }
        plain.to_owned()
    };
    (!value.is_empty()).then_some(value)
}
