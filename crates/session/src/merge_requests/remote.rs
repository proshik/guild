//! A git remote's URL read as a forge's host and project path — GLD-063.

/// Where a remote points: the host and the project's path on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Remote {
    /// The host without user info — with an https remote's port, as glab
    /// names its hosts — or an ssh alias a glab host's `ssh_host` may name.
    pub host: String,
    /// The project's path, `group/sub/project`, without `.git`.
    pub project: String,
    /// Reached over ssh — an `ssh://` or scp-style URL — rather than http:
    /// only then may a glab host's `ssh_host` name it (code review r2,
    /// arch+quality-3).
    pub ssh: bool,
}

impl Remote {
    /// The host and project of an https, ssh or scp-like URL; `None` for a
    /// local path, a `file:` URL or anything without both.
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let mut web = false;
        // scp-like unless a scheme says otherwise.
        let mut ssh = true;
        let (authority, path) = if let Some((scheme, rest)) = url.split_once("://") {
            if !matches!(scheme, "https" | "http" | "ssh" | "git" | "git+ssh") {
                return None;
            }
            web = matches!(scheme, "https" | "http");
            ssh = matches!(scheme, "ssh" | "git+ssh");
            rest.split_once('/')?
        } else {
            // scp-like: `[user@]host:path`, never a path with a slash before
            // the colon.
            let (authority, path) = url.split_once(':')?;
            if authority.contains('/') || authority.is_empty() {
                return None;
            }
            (authority, path)
        };
        let authority = authority.rsplit('@').next()?;
        // An https remote's port is part of the authority glab keys its
        // host by (`glrepo.FromURL` keeps `u.Host`); an ssh port is the
        // transport's, not the API's (code review r1, arch+quality-3).
        let (host, port) = match authority.strip_prefix('[') {
            // A bracketed IPv6 host keeps its brackets out.
            Some(inner) => {
                let (host, rest) = inner.split_once(']')?;
                (host, rest.strip_prefix(':'))
            }
            None => match authority.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            },
        };
        let port = port.filter(|port| web && !port.is_empty());
        if host.is_empty()
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b':'))
        {
            return None;
        }
        let project = path.trim_matches('/');
        let project = project.strip_suffix(".git").unwrap_or(project);
        let project = project.trim_matches('/');
        if project.is_empty() || !project.contains('/') {
            return None;
        }
        let host = host.to_ascii_lowercase();
        Some(Self {
            host: match port {
                Some(port) => format!("{host}:{port}"),
                None => host,
            },
            project: project.to_owned(),
            ssh,
        })
    }
}

/// Whether a host looks like a GitLab: `gitlab` in one of its dot-separated
/// labels — `gitlab.example.com`, `my-gitlab.corp`. Used only to decide
/// whether a missing glab is worth saying.
pub fn looks_like_gitlab(host: &str) -> bool {
    host.to_ascii_lowercase()
        .split('.')
        .any(|label| label.contains("gitlab"))
}
