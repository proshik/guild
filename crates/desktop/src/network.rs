//! Targeted proxy adapters. Generic process owners never inject network settings.
use guild_session::network::{Mode, Network, Proxy, Scope};
use std::{
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

static CURRENT: OnceLock<Mutex<(u64, Network)>> = OnceLock::new();
fn current() -> &'static Mutex<(u64, Network)> {
    CURRENT.get_or_init(|| Mutex::new((0, Network::default())))
}
pub(crate) fn publish(network: &Network) {
    let mut value = current().lock().unwrap();
    if value.1 != *network {
        value.0 += 1;
        value.1 = network.clone();
    }
}
pub(crate) fn generation() -> u64 {
    current().lock().unwrap().0
}
pub(crate) fn snapshot() -> Network {
    current().lock().unwrap().1.clone()
}
pub(crate) fn policy(scope: Scope) -> Result<Option<Proxy>, &'static str> {
    snapshot().policy(scope)
}
pub(crate) fn apply(command: &mut Command, scope: Scope) -> Result<(), &'static str> {
    apply_policy(command, policy(scope)?.as_ref());
    Ok(())
}
pub(crate) fn apply_policy(command: &mut Command, proxy: Option<&Proxy>) {
    if let Some(proxy) = proxy {
        for (key, value) in proxy.environment() {
            if let Some(value) = value {
                command.env(key, value);
            } else {
                command.env_remove(key);
            }
        }
    }
}
pub(crate) fn apply_pty(
    command: &mut portable_pty::CommandBuilder,
    scope: Scope,
) -> anyhow::Result<()> {
    if let Some(proxy) = policy(scope).map_err(anyhow::Error::msg)? {
        for (key, value) in proxy.environment() {
            if let Some(value) = value {
                command.env(key, value);
            } else {
                command.env_remove(key);
            }
        }
    }
    Ok(())
}
pub(crate) fn apply_pty_environment(
    command: &mut portable_pty::CommandBuilder,
    environment: &guild_session::network::Environment,
) {
    for (key, value) in environment {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
}
/// Every HTTP consumer retains its own timeouts and response handling.
pub(crate) fn http_policy(network: &Network) -> anyhow::Result<Option<ureq::Proxy>> {
    let Some(proxy) = network.policy(Scope::Guild).map_err(anyhow::Error::msg)? else {
        return Ok(ureq::Proxy::try_from_env());
    };
    let host = if proxy.host.contains(':') {
        format!("[{}]", proxy.host)
    } else {
        proxy.host.clone()
    };
    let mut builder = ureq::Proxy::builder(ureq::ProxyProtocol::Http)
        .host(&host)
        .port(proxy.port);
    for entry in &proxy.bypass {
        builder = builder.no_proxy(entry);
        if entry.parse::<std::net::Ipv6Addr>().is_ok() {
            builder = builder.no_proxy(&format!("[{entry}]"));
        }
    }
    builder
        .build()
        .map(Some)
        .map_err(|_| anyhow::anyhow!("Invalid proxy configuration"))
}

/// Match Custom bypass numerically/canonically, but preserve the destination URI
/// byte-for-byte for resolution, TLS and HTTP. The pinned enabled stack uses the
/// public ureq TCP/Rustls connectors; only the proxy-connect stage is bypassed.
#[derive(Debug)]
pub(crate) struct HttpConnector {
    custom: Option<Proxy>,
    inner: ureq::unversioned::transport::DefaultConnector,
}
pub(crate) fn http_connector(network: &Network) -> anyhow::Result<HttpConnector> {
    Ok(HttpConnector {
        custom: network.policy(Scope::Guild).map_err(anyhow::Error::msg)?,
        inner: Default::default(),
    })
}
impl ureq::unversioned::transport::Connector<()> for HttpConnector {
    type Out = Box<dyn ureq::unversioned::transport::Transport>;
    fn connect(
        &self,
        details: &ureq::unversioned::transport::ConnectionDetails,
        chained: Option<()>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        use ureq::unversioned::transport::{
            ConnectionDetails, RustlsConnector, TcpConnector, Transport,
        };
        if details.config.proxy().is_some()
            && self
                .custom
                .as_ref()
                .is_some_and(|p| p.bypasses(details.uri.host().unwrap_or("")))
        {
            let addrs = if details.addrs.is_empty() {
                details
                    .resolver
                    .resolve(details.uri, details.config, details.timeout)?
            } else {
                details.addrs.clone()
            };
            let direct = ConnectionDetails {
                uri: details.uri,
                addrs,
                config: details.config,
                request_level: details.request_level,
                resolver: details.resolver,
                now: details.now,
                timeout: details.timeout,
                current_time: details.current_time.clone(),
                run_connector: details.run_connector.clone(),
            };
            return TcpConnector::default()
                .chain(RustlsConnector::default())
                .connect(&direct, chained)
                .map(|transport| transport.map(Transport::boxed));
        }
        self.inner.connect(details, chained)
    }
}
/// Construct a consumer without changing its timeout/redirect/auth configuration.
pub(crate) fn http_agent(
    config: ureq::config::Config,
    network: &Network,
) -> anyhow::Result<ureq::Agent> {
    Ok(ureq::Agent::with_parts(
        config,
        http_connector(network)?,
        ureq::unversioned::resolver::DefaultResolver::default(),
    ))
}

/// Captured applied Custom identity; diagnostics never consult mutable settings.
#[derive(Default, Clone)]
pub(crate) struct GitPolicy(Option<Proxy>);
impl GitPolicy {
    pub(crate) fn redact(&self, text: &str) -> String {
        let Some(proxy) = &self.0 else {
            return text.to_owned();
        };
        let mut redacted = text.to_owned();
        // Git/libcurl may normalize the hostname's case. Retain unrelated
        // diagnostics, replacing only this invocation's applied URL/host.
        for needle in [proxy.url(), proxy.host.clone()] {
            let lower = redacted.to_ascii_lowercase();
            let mut next = String::new();
            let mut end = 0;
            for (start, _) in lower.match_indices(&needle.to_ascii_lowercase()) {
                next.push_str(&redacted[end..start]);
                next.push_str("[proxy]");
                end = start + needle.len();
            }
            next.push_str(&redacted[end..]);
            redacted = next;
        }
        redacted
    }
}

/// HTTPS Git configuration is private to an invocation, never a config-file edit.
pub(crate) fn git(
    command: &mut Command,
    root: &std::path::Path,
    target: &str,
    push: bool,
    remote: bool,
) -> Result<GitPolicy, crate::git::GitError> {
    git_policy(command, root, target, push, remote, &snapshot())
}
fn git_policy(
    command: &mut Command,
    root: &std::path::Path,
    target: &str,
    push: bool,
    remote: bool,
    network: &Network,
) -> Result<GitPolicy, crate::git::GitError> {
    use crate::git::{GitError, run_with_env};
    if network.mode == Mode::System || !network.scopes[Scope::Git as usize] {
        return Ok(GitPolicy::default());
    }
    let urls = if remote {
        let mut args = vec!["remote", "get-url"];
        if push {
            args.push("--push");
        }
        args.extend(["--all", target]);
        String::from_utf8(run_with_env(root, &args, Some(command))?)
            .map_err(|_| GitError::Failed("Cannot read Git remote URL".into()))?
    } else {
        String::from_utf8(run_with_env(
            root,
            &["ls-remote", "--get-url", "--", target],
            Some(command),
        )?)
        .map_err(|_| GitError::Failed("Cannot read Git URL".into()))?
    };
    if !urls.lines().any(|url| url.starts_with("https://")) {
        return Ok(GitPolicy::default());
    }
    let proxy = network.custom().map_err(|e| GitError::Failed(e.into()))?;
    // URL-scoped config does not change SSH, even for a remote with mixed push URLs.
    let mut entries = Vec::new();
    for url in urls.lines() {
        if !url.starts_with("https://") {
            continue;
        }
        gpui::http_client::Url::parse(url)
            .map_err(|_| GitError::Failed("Invalid HTTPS Git URL".into()))?;
        // Keep a proxy available after redirects; libcurl evaluates NO_PROXY
        // for each destination instead of pinning the initial URL to direct.
        entries.push((format!("http.{url}.proxy"), proxy.url()));
        if remote {
            entries.push((format!("remote.{target}.proxy"), proxy.url()));
        }
    }
    if entries.is_empty() {
        return Ok(GitPolicy::default());
    }
    if !remote {
        let name = String::from_utf8(run_with_env(
            root,
            &[
                "config",
                "--default",
                "origin",
                "--get",
                "clone.defaultRemoteName",
            ],
            Some(command),
        )?)
        .map_err(|_| GitError::Failed("Cannot read clone remote name".into()))?;
        entries.push((format!("remote.{}.proxy", name.trim()), proxy.url()));
    }
    // Retain the caller's counted entries; append policy with command-line priority.
    let inherited = command
        .get_envs()
        .find(|(k, _)| *k == "GIT_CONFIG_COUNT")
        .and_then(|(_, v)| v.map(|v| v.to_owned()))
        .or_else(|| std::env::var_os("GIT_CONFIG_COUNT"));
    let count = match inherited {
        Some(n) => n
            .to_str()
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|n| *n < 4096)
            .ok_or_else(|| GitError::Failed("Invalid inherited Git configuration count".into()))?,
        None => 0,
    };
    command.env("GIT_CONFIG_COUNT", (count + entries.len()).to_string());
    for (index, (key, value)) in entries.into_iter().enumerate() {
        command
            .env(format!("GIT_CONFIG_KEY_{}", count + index), key)
            .env(format!("GIT_CONFIG_VALUE_{}", count + index), value);
    }
    // NO_PROXY is for this libcurl consumer only, with the same canonical bypass.
    command
        .env("NO_PROXY", proxy.no_proxy())
        .env("no_proxy", proxy.no_proxy());
    Ok(GitPolicy(Some(proxy)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Status(u16),
    Authentication,
    SystemCredentials,
    SystemTransport,
    Timeout,
    Dns,
    Refused,
    Tls,
    Unavailable,
    Failed,
    Invalid(&'static str),
}
#[derive(Default)]
pub(crate) struct Check {
    worker: Option<std::thread::JoinHandle<Outcome>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) result: Option<(Network, Outcome)>,
    checked: Network,
    pub(crate) route: Option<bool>,
    valid: bool,
}
impl Check {
    pub(crate) fn matches(&self, network: &Network) -> bool {
        self.checked == *network
    }
    pub(crate) fn active(&self) -> bool {
        self.worker.is_some()
    }
    pub(crate) fn invalidate(&mut self) {
        self.valid = false;
        self.result = None;
    }
    pub(crate) fn start(&mut self, network: Network) {
        if self.active() {
            return;
        }
        self.route = check_route(&network).ok().map(|route| route.is_some());
        self.valid = true;
        self.result = None;
        self.checked = network.clone();
        self.stop = Default::default();
        let stop = self.stop.clone();
        self.worker = Some(std::thread::spawn(move || {
            diagnose(&network, &stop, "/usr/bin/curl", Duration::from_secs(10))
        }));
    }
    pub(crate) fn poll(&mut self) -> bool {
        if self.worker.as_ref().is_some_and(|t| t.is_finished()) {
            let outcome = self
                .worker
                .take()
                .unwrap()
                .join()
                .unwrap_or(Outcome::Failed);
            self.result = self.valid.then(|| (self.checked.clone(), outcome));
            true
        } else {
            false
        }
    }
}
impl Drop for Check {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(t) = self.worker.take() {
            let _ = t.join();
        }
    }
}
fn check_route(network: &Network) -> Result<Option<String>, Outcome> {
    if network.mode != Mode::System {
        let proxy = network.custom().map_err(Outcome::Invalid)?;
        return Ok((!proxy.bypasses("api.anthropic.com")).then(|| proxy.url()));
    }
    system_route(ureq::Proxy::try_from_env())
}
fn system_route(proxy: Option<ureq::Proxy>) -> Result<Option<String>, Outcome> {
    let Some(proxy) = proxy else {
        return Ok(None);
    };
    let uri = "https://api.anthropic.com/".parse().unwrap();
    if proxy.is_no_proxy(&uri) {
        return Ok(None);
    }
    if proxy.username().is_some() || proxy.password().is_some() {
        return Err(Outcome::SystemCredentials);
    }
    match proxy.protocol() {
        ureq::ProxyProtocol::Http | ureq::ProxyProtocol::Https => Ok(Some(proxy.uri().to_string())),
        _ => Err(Outcome::SystemTransport),
    }
}
fn diagnose(
    network: &Network,
    stop: &std::sync::atomic::AtomicBool,
    binary: &str,
    budget: Duration,
) -> Outcome {
    use std::io::{Read, Write};
    use std::process::Stdio;
    use std::sync::atomic::Ordering;
    let proxy = match check_route(network) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let proxy = proxy.unwrap_or_default();
    if proxy.chars().any(|c| c.is_control()) {
        return Outcome::Invalid("Invalid proxy address");
    }
    let escaped = proxy.replace('\\', "\\\\").replace('"', "\\\"");
    let config = format!("proxy = \"{escaped}\"\nnoproxy = \"\"\n");
    if config.len() > 8192 {
        return Outcome::Invalid("Proxy address is too long");
    }
    let started = Instant::now();
    let mut command = Command::new(binary);
    command
        .env_clear()
        .args([
            "--disable",
            "--config",
            "-",
            "--head",
            "--silent",
            "--max-time",
            "10",
            "--max-redirs",
            "0",
            "--proto",
            "=https",
            "--output",
            "/dev/null",
            "--write-out",
            "%{http_code} %{http_connect}",
            "https://api.anthropic.com/",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = match guild_session::process::ProcessGroup::spawn(&mut command) {
        Ok(c) => c,
        Err(_) => return Outcome::Unavailable,
    };
    let sent = child
        .take_stdin()
        .is_some_and(|mut input| input.write_all(config.as_bytes()).is_ok());
    let stdout = child.take_stdout().unwrap();
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        stdout.take(65).read_to_string(&mut out).map(|_| out)
    });
    let status = loop {
        if !sent || stop.load(Ordering::Acquire) || started.elapsed() >= budget {
            let _ = child.stop();
            break None;
        }
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.stop();
                break None;
            }
        }
    };
    let output = reader.join().ok().and_then(Result::ok).unwrap_or_default();
    let Some(status) = status else {
        return Outcome::Timeout;
    };
    let numbers = output
        .split_whitespace()
        .map(str::parse::<u16>)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    if numbers.len() != 2 {
        return Outcome::Failed;
    }
    if numbers[1] == 407 {
        return Outcome::Authentication;
    }
    if status.success() && numbers[0] >= 100 {
        return Outcome::Status(numbers[0]);
    }
    match status.code() {
        Some(5 | 6) => Outcome::Dns,
        Some(7) => Outcome::Refused,
        Some(28) => Outcome::Timeout,
        Some(35 | 51 | 60) => Outcome::Tls,
        _ => Outcome::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_environment_is_explicit_and_off_is_untouched() {
        let n = Network {
            mode: Mode::Custom,
            address: "http://127.0.0.1:8080".into(),
            ..Default::default()
        };
        let mut c = Command::new("unused");
        c.env("ALL_PROXY", "old").env("KEEP", "yes");
        apply_policy(&mut c, Some(&n.custom().unwrap()));
        let vars = c.get_envs().collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(vars[std::ffi::OsStr::new("ALL_PROXY")], None);
        assert_eq!(vars[std::ffi::OsStr::new("KEEP")].unwrap(), "yes");
        let mut c = Command::new("unused");
        c.env("ALL_PROXY", "old");
        apply_policy(&mut c, None);
        assert_eq!(c.get_envs().next().unwrap().1.unwrap(), "old");
    }
    #[test]
    fn diagnostic_parent_deadline_reaps_owned_child() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("curl");
        std::fs::write(&script, "#!/bin/sh\ncat >/dev/null\nsleep 60 &\nwait\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let n = Network {
            mode: Mode::Custom,
            address: "http://127.0.0.1:8080".into(),
            ..Default::default()
        };
        let start = Instant::now();
        assert_eq!(
            diagnose(
                &n,
                &false.into(),
                script.to_str().unwrap(),
                Duration::from_millis(100)
            ),
            Outcome::Timeout
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use std::io::{Read, Write};
    fn settings(port: u16) -> Network {
        Network {
            mode: Mode::Custom,
            address: format!("http://127.0.0.1:{port}"),
            ..Default::default()
        }
    }
    fn listener() -> std::net::TcpListener {
        std::net::TcpListener::bind("127.0.0.1:0").unwrap()
    }
    fn headers(s: &mut std::net::TcpStream) -> String {
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut out = Vec::new();
        while !out.ends_with(b"\r\n\r\n") && out.len() < 8192 {
            let mut b = [0; 1];
            if s.read(&mut b).unwrap_or(0) == 0 {
                break;
            }
            out.push(b[0]);
        }
        String::from_utf8(out).unwrap()
    }
    #[test]
    fn real_http_routes_to_proxy_and_bypasses_local_origin() {
        let proxy = listener();
        let port = proxy.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = proxy.accept().unwrap();
            let mut first = headers(&mut stream);
            assert!(!first.to_lowercase().contains("proxy-authorization"));
            if first.starts_with("CONNECT ") {
                stream
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .unwrap();
                first = headers(&mut stream);
            }
            assert!(first.contains("guild-network.invalid"));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nfixture",
                )
                .unwrap();
        });
        let config = ureq::Agent::config_builder()
            .proxy(http_policy(&settings(port)).unwrap())
            .timeout_global(Some(Duration::from_secs(2)))
            .build();
        let agent: ureq::Agent = config.into();
        assert_eq!(
            agent
                .get("http://guild-network.invalid/image")
                .call()
                .unwrap()
                .body_mut()
                .read_to_string()
                .unwrap(),
            "fixture"
        );
        worker.join().unwrap();
        let origin = listener();
        let url = format!("http://{}/image", origin.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut s, _) = origin.accept().unwrap();
            assert!(headers(&mut s).starts_with("GET "));
            s.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                .unwrap();
        });
        assert_eq!(agent.get(&url).call().unwrap().status(), 204);
        worker.join().unwrap();
    }
    #[test]
    fn real_curl_reports_proxy_authentication_without_credentials() {
        let proxy = listener();
        let port = proxy.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let (mut s, _) = proxy.accept().unwrap();
            let request = headers(&mut s);
            assert!(request.starts_with("CONNECT api.anthropic.com:443"));
            assert!(!request.to_lowercase().contains("proxy-authorization"));
            s.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=fixture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        assert_eq!(
            diagnose(
                &settings(port),
                &false.into(),
                "/usr/bin/curl",
                Duration::from_secs(2)
            ),
            Outcome::Authentication
        );
        worker.join().unwrap();
        let mut n = settings(port);
        n.address = "http://fake:secret@127.0.0.1:1234".into();
        assert!(matches!(
            diagnose(&n, &false.into(), "/missing", Duration::from_millis(50)),
            Outcome::Invalid(_)
        ));
    }
    #[test]
    fn actual_child_captures_environment_at_launch() {
        let mut old = Command::new("/bin/sh");
        old.args([
            "-c",
            "sleep 0.1; printf %s \"$HTTPS_PROXY|${ALL_PROXY-unset}|$KEEP\"",
        ])
        .env("KEEP", "kept");
        apply_policy(&mut old, Some(&settings(18080).custom().unwrap()));
        let child = old.stdout(std::process::Stdio::piped()).spawn().unwrap();
        let mut new = Command::new("/bin/sh");
        new.args(["-c", "printf %s \"$HTTPS_PROXY|${ALL_PROXY-unset}|$KEEP\""])
            .env("KEEP", "kept");
        apply_policy(&mut new, Some(&settings(18081).custom().unwrap()));
        assert_eq!(
            String::from_utf8(new.output().unwrap().stdout).unwrap(),
            "http://127.0.0.1:18081|unset|kept"
        );
        assert_eq!(
            String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap(),
            "http://127.0.0.1:18080|unset|kept"
        );
    }
    #[test]
    fn real_git_https_overrides_proxy_and_leaves_ssh_untouched() {
        let proxy = listener();
        let port = proxy.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();
        let url = "https://guild-network.invalid/repo.git";
        let worker = std::thread::spawn(move || {
            let (mut s, _) = proxy.accept().unwrap();
            assert!(headers(&mut s).starts_with("CONNECT guild-network.invalid:443"));
            s.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let mut command = Command::new("git");
        command
            .args(["ls-remote", url])
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.proxy")
            .env("GIT_CONFIG_VALUE_0", "http://127.0.0.1:1")
            .env("GIT_TERMINAL_PROMPT", "0");
        git_policy(&mut command, dir.path(), url, false, false, &settings(port)).unwrap();
        assert!(!command.output().unwrap().status.success());
        worker.join().unwrap();
        let mut ssh = Command::new("unused");
        ssh.env("GIT_SSH_COMMAND", "fixture-ssh");
        let mut invalid = settings(port);
        invalid.address.clear();
        git_policy(
            &mut ssh,
            dir.path(),
            "ssh://host/repo",
            false,
            false,
            &invalid,
        )
        .unwrap();
        assert_eq!(ssh.get_envs().count(), 1);
    }
}

#[cfg(test)]
mod correction_tests {
    use super::*;
    use std::io::{Read, Write};
    use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
    use ureq::unversioned::transport::NextTimeout;
    #[test]
    fn ipv6_proxy_adapter_preserves_bypass() {
        let n = Network {
            mode: Mode::Custom,
            address: "http://[::1]:8080".into(),
            bypass: "internal.test,2001:0db8:0:0:0:0:0:7".into(),
            ..Default::default()
        };
        let proxy = http_policy(&n).expect("valid IPv6 proxy adapter").unwrap();
        assert_eq!(proxy.uri().to_string(), "http://[::1]:8080/");
        assert!(proxy.is_no_proxy(&"http://internal.test/".parse().unwrap()));
        assert!(proxy.is_no_proxy(&"http://[2001:db8::7]/".parse().unwrap()));
    }
    #[derive(Debug)]
    struct LocalResolver;
    impl Resolver for LocalResolver {
        fn resolve(
            &self,
            uri: &ureq::http::Uri,
            config: &ureq::config::Config,
            timeout: NextTimeout,
        ) -> Result<ResolvedSocketAddrs, ureq::Error> {
            assert_ne!(
                uri.host(),
                Some("localhost"),
                "absolute dotted destination must reach fixture resolver intact"
            );
            assert_ne!(
                uri.host(),
                Some("api.internal.test"),
                "absolute dotted destination must reach resolver intact"
            );
            if uri.host().is_some_and(|h| {
                h.ends_with("internal.test")
                    || h.ends_with("internal.test.")
                    || h == "[2001:db8::7]"
                    || h == "localhost."
            }) {
                let mut out = DefaultResolver::default().empty();
                out.push(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    uri.port_u16().unwrap(),
                )));
                Ok(out)
            } else {
                DefaultResolver::default().resolve(uri, config, timeout)
            }
        }
    }
    fn serve(listener: std::net::TcpListener, reply: String) -> std::thread::JoinHandle<bool> {
        listener.set_nonblocking(true).unwrap();
        std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(6);
            while Instant::now() < until {
                if let Ok((mut s, _)) = listener.accept() {
                    // macOS accepted sockets can inherit the listener's O_NONBLOCK.
                    s.set_nonblocking(false).unwrap();
                    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    let read_headers = |s: &mut std::net::TcpStream| {
                        let mut req = Vec::new();
                        while !req.ends_with(b"\r\n\r\n") && req.len() < 8192 {
                            let mut b = [0];
                            if s.read(&mut b).unwrap_or(0) == 0 {
                                break;
                            }
                            req.push(b[0]);
                        }
                        req
                    };
                    let req = read_headers(&mut s);
                    assert!(
                        req.ends_with(b"\r\n\r\n"),
                        "fixture received incomplete headers"
                    );
                    if req.starts_with(b"CONNECT ") && reply.starts_with("HTTP/1.1 204") {
                        s.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                            .unwrap();
                        let request = read_headers(&mut s);
                        assert!(
                            request.starts_with(b"GET ") && request.ends_with(b"\r\n\r\n"),
                            "fixture must receive destination GET before replying, received {:?}",
                            String::from_utf8_lossy(&request).lines().next()
                        );
                    }
                    let _ = s.write_all(reply.as_bytes());
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        })
    }
    #[test]
    fn actual_http_bypass_and_redirect_keep_canonical_routes() {
        for host in ["[::1]", "[2001:db8::7]", "localhost.", "api.internal.test."] {
            let origin = std::net::TcpListener::bind(if host == "[::1]" {
                "[::1]:0"
            } else {
                "127.0.0.1:0"
            })
            .unwrap();
            let port = origin.local_addr().unwrap().port();
            let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let pp = proxy.local_addr().unwrap().port();
            let direct = serve(
                origin,
                "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into(),
            );
            let spy = serve(
                proxy,
                "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            );
            let n = Network {
                mode: Mode::Custom,
                address: format!("http://127.0.0.1:{pp}"),
                bypass: "internal.test,2001:0db8:0:0:0:0:0:7".into(),
                ..Default::default()
            };
            let config = ureq::Agent::config_builder()
                .proxy(http_policy(&n).unwrap())
                .timeout_global(Some(Duration::from_secs(5)))
                .http_status_as_error(false)
                .build();
            // Production connector supplied by http_agent, with only DNS isolated to loopback.
            let configured = http_agent(config, &n).unwrap();
            let agent = ureq::Agent::with_parts(
                configured.config().clone(),
                http_connector(&n).unwrap(),
                LocalResolver,
            );
            let outcome = agent
                .get(format!("http://{host}:{port}/image"))
                .call()
                .map(|r| r.status().as_u16());
            let hit = direct.join().unwrap();
            let proxy_hit = spy.join().unwrap();
            assert_eq!(outcome.unwrap(), 204, "{host}");
            assert!(hit);
            assert!(!proxy_hit, "bypass contacted proxy for {host}");
        }
        // Redirect begins at bypassed origin but the next host must use Custom.
        let origin = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = origin.local_addr().unwrap().port();
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let pp = proxy.local_addr().unwrap().port();
        let direct=serve(origin,"HTTP/1.1 302 Found\r\nLocation: http://external.invalid/image\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into());
        let proxied = serve(
            proxy,
            "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into(),
        );
        let n = Network {
            mode: Mode::Custom,
            address: format!("http://127.0.0.1:{pp}"),
            ..Default::default()
        };
        let config = ureq::Agent::config_builder()
            .proxy(http_policy(&n).unwrap())
            .timeout_global(Some(Duration::from_secs(5)))
            .build();
        let outcome = http_agent(config, &n)
            .unwrap()
            .get(format!("http://127.0.0.1:{port}/"))
            .call()
            .map(|r| r.status().as_u16());
        assert!(matches!(outcome, Ok(204)), "redirect outcome: {outcome:?}");
        assert!(direct.join().unwrap());
        assert!(proxied.join().unwrap());
    }
    #[test]
    fn redirect_to_expanded_loopback_does_not_contact_proxy() {
        let origin = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let first = origin.local_addr().unwrap();
        let end = std::net::TcpListener::bind("[::1]:0").unwrap();
        let port = end.local_addr().unwrap().port();
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let pp = proxy.local_addr().unwrap().port();
        let start = serve(
            origin,
            format!(
                "HTTP/1.1 302 Found\r\nLocation: http://[0:0:0:0:0:0:0:1]:{port}/image\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
        );
        let direct = serve(
            end,
            "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into(),
        );
        let spy = serve(
            proxy,
            "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        );
        let n = Network {
            mode: Mode::Custom,
            address: format!("http://127.0.0.1:{pp}"),
            ..Default::default()
        };
        let cfg = ureq::Agent::config_builder()
            .proxy(http_policy(&n).unwrap())
            .timeout_global(Some(Duration::from_secs(5)))
            .build();
        let result = http_agent(cfg, &n)
            .unwrap()
            .get(format!("http://{first}/"))
            .call()
            .map(|r| r.status().as_u16());
        assert!(start.join().unwrap());
        let hit = direct.join().unwrap();
        let proxy_hit = spy.join().unwrap();
        assert_eq!(result.unwrap(), 204);
        assert!(hit);
        assert!(!proxy_hit);
    }
    #[test]
    fn applied_git_diagnostics_hide_captured_proxy_only() {
        let dir = tempfile::tempdir().unwrap();
        let mut n = Network {
            mode: Mode::Custom,
            address: "http://fake-old-proxy.invalid:18080".into(),
            ..Default::default()
        };
        let mut c = Command::new("git");
        let applied = git_policy(
            &mut c,
            dir.path(),
            "https://example.invalid/repo",
            false,
            false,
            &n,
        )
        .unwrap();
        // Later settings cannot change the identity retained by this operation.
        n.address = "http://new-proxy.invalid:18081".into();
        let mut child = Command::new("/bin/sh");
        child.args(["-c","printf 'Could not resolve proxy: FAKE-OLD-PROXY.invalid\\nUnrelated useful diagnostic\\n' >&2"]);
        let output = child.output().unwrap();
        let cleaned = applied.redact(&String::from_utf8_lossy(&output.stderr));
        assert!(
            !cleaned
                .to_ascii_lowercase()
                .contains("fake-old-proxy.invalid")
        );
        assert!(cleaned.contains("Could not resolve proxy"));
        assert!(cleaned.contains("Unrelated useful diagnostic"));
        assert_eq!(
            GitPolicy::default().redact("http://unchanged.invalid"),
            "http://unchanged.invalid"
        );
    }
    #[test]
    fn system_check_limitations_are_distinct_from_invalid_custom() {
        let credentials = system_route(Some(
            ureq::Proxy::new("http://fake:dummy@proxy.invalid:8080").unwrap(),
        ))
        .unwrap_err();
        assert_eq!(format!("{credentials:?}"), "SystemCredentials");
        let transport = system_route(Some(
            ureq::Proxy::new("socks5://proxy.invalid:1080").unwrap(),
        ))
        .unwrap_err();
        assert_eq!(format!("{transport:?}"), "SystemTransport");
    }
    #[test]
    fn actual_https_clone_overrides_origin_and_configured_remote() {
        for remote in ["origin", "upstream"] {
            let dir = tempfile::tempdir().unwrap();
            let config = dir.path().join("config");
            let bytes = format!("[remote \"{remote}\"]\n proxy = http://127.0.0.1:1\n");
            std::fs::write(&config, &bytes).unwrap();
            let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = proxy.local_addr().unwrap().port();
            let hit=serve(proxy,"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into());
            let n = Network {
                mode: Mode::Custom,
                address: format!("http://127.0.0.1:{port}"),
                ..Default::default()
            };
            let url = "https://guild-network.invalid/repo.git";
            let mut c = Command::new("git");
            c.current_dir(dir.path())
                .env("GIT_CONFIG_GLOBAL", &config)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_TERMINAL_PROMPT", "0");
            if remote != "origin" {
                c.env("GIT_CONFIG_COUNT", "1")
                    .env("GIT_CONFIG_KEY_0", "clone.defaultRemoteName")
                    .env("GIT_CONFIG_VALUE_0", remote);
            }
            let applied = git_policy(&mut c, dir.path(), url, false, false, &n).unwrap();
            c.args(
                crate::git::Write::Clone {
                    url: url.into(),
                    destination: dir.path().join("destination"),
                }
                .args(),
            );
            let output = c.output().unwrap();
            assert!(hit.join().unwrap(), "clone did not use Custom for {remote}");
            assert!(!output.status.success());
            let err = applied.redact(&String::from_utf8_lossy(&output.stderr));
            assert!(!err.contains("127.0.0.1"));
            assert_eq!(std::fs::read_to_string(&config).unwrap(), bytes);
        }
    }
    #[test]
    fn clone_uses_effective_remote_and_never_pins_direct() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        let bytes=b"[clone]\n defaultRemoteName = upstream\n[remote \"upstream\"]\n proxy = http://old.invalid:9999\n";
        std::fs::write(&config, bytes).unwrap();
        let n = Network {
            mode: Mode::Custom,
            address: "http://127.0.0.1:8080".into(),
            bypass: "internal.test".into(),
            ..Default::default()
        };
        let mut c = Command::new("git");
        c.env("GIT_CONFIG_GLOBAL", &config)
            .env("GIT_CONFIG_NOSYSTEM", "1");
        git_policy(
            &mut c,
            dir.path(),
            "https://internal.test/repo",
            false,
            false,
            &n,
        )
        .unwrap();
        c.args(["config", "--get", "remote.upstream.proxy"]);
        assert_eq!(
            String::from_utf8(c.output().unwrap().stdout)
                .unwrap()
                .trim(),
            n.custom().unwrap().url()
        );
        let vars = c
            .get_envs()
            .filter_map(|(k, v)| {
                v.map(|v| {
                    (
                        k.to_string_lossy().into_owned(),
                        v.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        for (k, v) in &vars {
            if k.starts_with("GIT_CONFIG_KEY_") && v.starts_with("http.") {
                let i = k.trim_start_matches("GIT_CONFIG_KEY_");
                assert_eq!(
                    vars[&format!("GIT_CONFIG_VALUE_{i}")],
                    n.custom().unwrap().url()
                );
            }
        }
        assert_eq!(std::fs::read(config).unwrap(), bytes);
    }
}
