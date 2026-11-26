//! Validated URL clone input. Filesystem checks happen on submission, never paint.
use std::path::{Path, PathBuf};

/// The native single-line form, with a suggestion until the person edits it.
#[derive(Debug)]
pub struct Form {
    /// Repository URL, as typed.
    pub url: String,
    /// Full destination, as typed.
    pub directory: String,
    pub(crate) parent: PathBuf,
    manual: bool,
}
impl Form {
    /// Start beside the last project, or under the home directory.
    pub fn new(parent: &Path) -> Self {
        Self {
            url: String::new(),
            directory: parent.display().to_string(),
            parent: parent.into(),
            manual: false,
        }
    }
    /// Update the suggestion only while it still belongs to the form.
    pub fn set_url(&mut self, url: String) {
        self.url = url;
        if !self.manual {
            self.directory = repository_name(&self.url)
                .map(|name| self.parent.join(name).display().to_string())
                .unwrap_or_else(|_| self.parent.display().to_string());
        }
    }
    /// A manual destination survives subsequent URL edits.
    pub fn set_directory(&mut self, directory: String) {
        self.manual = true;
        self.directory = directory;
    }
    /// Input validity without filesystem reads, including during rendering.
    pub fn input_error(&self) -> Option<String> {
        repository_name(&self.url)
            .and_then(|_| validate_destination_name(Path::new(&self.directory)))
            .err()
    }
    /// Validate the request at submission time.
    pub fn validate(&self) -> Result<(), String> {
        repository_name(&self.url)?;
        validate_destination(Path::new(&self.directory))
    }
}

/// Prefer the last opened project's parent, then the active project's parent,
/// then the user's home. The desk persists its recent-project list.
pub fn default_parent(recent: Option<&Path>, active: Option<&Path>, home: &Path) -> PathBuf {
    recent
        .and_then(Path::parent)
        .or_else(|| active.and_then(Path::parent))
        .unwrap_or(home)
        .to_path_buf()
}

/// Accept only credential-free HTTPS and SSH, including scp-style SSH.
pub fn repository_name(input: &str) -> Result<String, String> {
    let url = input.trim();
    let invalid = || "Enter an HTTPS or SSH repository URL without passwords or tokens.".to_owned();
    if url.is_empty()
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
        || url.contains(['?', '#', '\\'])
    {
        return Err(invalid());
    }
    let path = if let Some(rest) = url.strip_prefix("https://") {
        let (host, path) = rest.split_once('/').ok_or_else(invalid)?;
        if host.contains('@') || !valid_host(host) {
            return Err(invalid());
        }
        path
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        let (authority, path) = rest.split_once('/').ok_or_else(invalid)?;
        let host = if let Some((user, host)) = authority.split_once('@') {
            if !valid_user(user) {
                return Err(invalid());
            }
            host
        } else {
            authority
        };
        if !valid_host(host) {
            return Err(invalid());
        }
        path
    } else {
        if url.contains("://") {
            return Err(invalid());
        }
        let (authority, path) = url.split_once(':').ok_or_else(invalid)?;
        let host = if let Some((user, host)) = authority.split_once('@') {
            if !valid_user(user) {
                return Err(invalid());
            }
            host
        } else {
            authority
        };
        if !valid_host(host) {
            return Err(invalid());
        }
        path
    };
    // Percent escapes are not decoded into local path components or credentials.
    if path.contains('%') || path.is_empty() {
        return Err(invalid());
    }
    let component = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .ok_or_else(invalid)?;
    let name = component.strip_suffix(".git").unwrap_or(component);
    if name.is_empty() || matches!(name, "." | "..") || name.starts_with('-') {
        return Err(invalid());
    }
    Ok(name.to_owned())
}
fn valid_user(user: &str) -> bool {
    !user.is_empty()
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}
fn valid_host(authority: &str) -> bool {
    let valid_port = |port: &str| port.parse::<u16>().is_ok_and(|port| port != 0);
    if let Some(rest) = authority.strip_prefix('[') {
        let Some((address, suffix)) = rest.split_once(']') else {
            return false;
        };
        return address.parse::<std::net::Ipv6Addr>().is_ok()
            && (suffix.is_empty() || suffix.strip_prefix(':').is_some_and(valid_port));
    }
    let host = if let Some((host, port)) = authority.split_once(':') {
        if !valid_port(port) {
            return false;
        }
        host
    } else {
        authority
    };
    let host = host.trim_end_matches('.');
    !host.is_empty()
        && host.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}
/// Validate names without accessing disk, for the form's enabled state.
pub fn validate_destination_name(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .file_name()
            .is_none_or(|name| name.to_string_lossy().trim().is_empty())
        || path.as_os_str().as_encoded_bytes().contains(&0)
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Choose an absolute destination with a new directory name.".into());
    }
    Ok(())
}
/// Refuse existing entries, including dangling symlinks, and missing parents.
pub fn validate_destination(path: &Path) -> Result<(), String> {
    validate_destination_name(path)?;
    match std::fs::symlink_metadata(path) {
        Ok(_) => return Err("The destination already exists. Choose a new directory.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let parent = path
        .parent()
        .ok_or("Choose an existing parent directory.")?;
    if !std::fs::metadata(parent)
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err("The parent is not a directory.".into());
    }
    Ok(())
}

/// Keyboard order for the form; every control has one place in the cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Url,
    Directory,
    Choose,
    Cancel,
    Submit,
}
impl Field {
    /// Wrap in either direction, including Shift-Tab from the URL.
    pub fn step(self, forward: bool) -> Self {
        let fields = [
            Self::Url,
            Self::Directory,
            Self::Choose,
            Self::Cancel,
            Self::Submit,
        ];
        let index = fields.iter().position(|field| *field == self).unwrap_or(0);
        fields[(index + if forward { 1 } else { fields.len() - 1 }) % fields.len()]
    }
}

/// One operation's state; a terminal result can only be consumed once.
#[derive(Debug)]
pub enum Phase {
    /// No process yet.
    Editing,
    /// Git is running.
    Running,
    /// Waiting for group cleanup after cancellation.
    Cancelling,
    /// Failure with optional retained private clone.
    Failed {
        message: String,
        retained: Option<PathBuf>,
    },
    /// Cancellation has finished.
    Cancelled { retained: Option<PathBuf> },
    /// Repository is already complete; retry opening, never cloning.
    Succeeded(PathBuf),
}
impl Phase {
    /// Whether the window still owns a running group.
    pub fn busy(&self) -> bool {
        matches!(self, Self::Running | Self::Cancelling)
    }
    /// Block duplicate starts and recloning a completed repository.
    pub fn start(&mut self) -> bool {
        if self.busy() || matches!(self, Self::Succeeded(_)) {
            return false;
        }
        *self = Self::Running;
        true
    }
    /// Cancellation is not completion until the process owner answers.
    pub fn cancel(&mut self) {
        if self.busy() {
            *self = Self::Cancelling;
        }
    }
    /// Ignore stale terminal results; publication wins a late cancellation.
    pub fn complete(&mut self, outcome: crate::git::clone::Outcome) -> bool {
        if !self.busy() {
            return false;
        }
        *self = match outcome {
            crate::git::clone::Outcome::Succeeded(path) => Self::Succeeded(path),
            crate::git::clone::Outcome::Failed { message, retained } => {
                Self::Failed { message, retained }
            }
            crate::git::clone::Outcome::Cancelled { retained } => Self::Cancelled { retained },
        };
        true
    }
}
