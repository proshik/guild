//! Global proxy preferences and pure, explicit request/child policy.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, net::IpAddr};

/// Connection source. System retains each consumer's current behavior.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Environment/configuration, or direct as before.
    #[default]
    System,
    /// The saved HTTP proxy for selected consumers.
    Custom,
    /// Preserve the rest of Settings when a future mode cannot be understood.
    #[serde(other)]
    Unsupported,
}
/// The purpose of a Guild-owned request or launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Scope {
    /// Usage, updates and external Markdown images.
    Guild,
    /// Interactive harness sessions.
    Agent,
    /// Ordinary shells.
    Terminal,
    /// Native HTTPS Git operations.
    Git,
    /// Run, language servers and dependency discovery: environment only.
    Tools,
}
/// Per-harness routing for newly launched agent processes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRoute {
    /// Preserve the global Agent sessions scope (including System/environment).
    #[default]
    Follow,
    /// Use the saved credential-free Custom address even when global scope is off.
    Proxy,
    /// Remove inherited proxy/bypass variables from this launched child.
    Direct,
    /// Reject a future unsupported choice without silently changing its route.
    #[serde(other)]
    Unsupported,
}
/// Explicit child environment Set/Remove intent, also restored after shell rc.
pub type Environment = Vec<(&'static str, Option<String>)>;
/// The supported proxy/bypass environment keys in both conventional cases.
pub const PROXY_KEYS: [&str; 8] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];
/// Persisted global settings. Debug never prints address/draft contents.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Network {
    /// Effective connection source.
    pub mode: Mode,
    /// Retained Custom draft, initially empty. Credential-like drafts are not saved.
    #[serde(serialize_with = "save_address")]
    pub address: String,
    /// Comma-separated additional bypass hosts/domains.
    pub bypass: String,
    /// Independent enabled flags, in Scope order.
    pub scopes: [bool; 5],
    /// Registry program names; absent choices follow the global scope.
    pub agents: BTreeMap<String, AgentRoute>,
}
impl Default for Network {
    fn default() -> Self {
        Self {
            mode: Mode::System,
            address: String::new(),
            bypass: String::new(),
            scopes: [true; 5],
            agents: BTreeMap::new(),
        }
    }
}
impl fmt::Debug for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Network")
            .field("mode", &self.mode)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}
fn save_address<S: serde::Serializer>(value: &str, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(if value.contains('@') { "" } else { value })
}
/// Validated credential-free HTTP proxy and canonical bypass entries.
#[derive(Clone, PartialEq, Eq)]
pub struct Proxy {
    /// Proxy host (IPv6 without brackets).
    pub host: String,
    /// Explicit nonzero TCP port.
    pub port: u16,
    /// Canonical bypass entries, including built-in local hosts.
    pub bypass: Vec<String>,
}
impl fmt::Debug for Proxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Proxy { .. }")
    }
}
impl Proxy {
    /// HTTP URL suitable for child environment or a request adapter.
    pub fn url(&self) -> String {
        format!(
            "http://{}:{}",
            if self.host.contains(':') {
                format!("[{}]", self.host)
            } else {
                self.host.clone()
            },
            self.port
        )
    }
    /// Environment spelling containing apex and dotted suffix rules.
    pub fn no_proxy(&self) -> String {
        self.bypass.join(",")
    }
    /// Whether a destination bypasses this proxy.
    pub fn bypasses(&self, host: &str) -> bool {
        matches_host(&self.bypass, host)
    }
    /// Explicit Set/Remove intent, retained separately from a process env map.
    pub fn environment(&self) -> Vec<(&'static str, Option<String>)> {
        vec![
            ("HTTP_PROXY", Some(self.url())),
            ("http_proxy", Some(self.url())),
            ("HTTPS_PROXY", Some(self.url())),
            ("https_proxy", Some(self.url())),
            ("NO_PROXY", Some(self.no_proxy())),
            ("no_proxy", Some(self.no_proxy())),
            ("ALL_PROXY", None),
            ("all_proxy", None),
        ]
    }
}
impl Network {
    /// Resolve a selected consumer; None means leave its configuration untouched.
    pub fn policy(&self, scope: Scope) -> Result<Option<Proxy>, &'static str> {
        if self.mode == Mode::System || !self.scopes[scope as usize] {
            return Ok(None);
        }
        self.custom().map(Some)
    }
    /// A missing registry entry preserves the pre-existing global agent behavior.
    pub fn agent_route(&self, harness: crate::HarnessKind) -> AgentRoute {
        self.agents
            .get(harness.program())
            .copied()
            .unwrap_or_default()
    }
    /// Capture the environment policy for one actual selected harness.
    pub fn agent_environment(
        &self,
        harness: crate::HarnessKind,
    ) -> Result<Environment, &'static str> {
        match self.agent_route(harness) {
            AgentRoute::Follow => Ok(self.policy(Scope::Agent)?.map(|p|p.environment()).unwrap_or_default()),
            AgentRoute::Proxy => self.custom().map(|p|p.environment()).map_err(|_| "This harness needs a valid saved HTTP proxy address and bypass in Settings → Network"),
            AgentRoute::Direct => Ok(PROXY_KEYS.into_iter().map(|key|(key,None)).collect()),
            AgentRoute::Unsupported => Err("Unsupported proxy choice for this harness in Settings → Network"),
        }
    }
    /// Validate Custom independently of switches (for Check connection).
    pub fn custom(&self) -> Result<Proxy, &'static str> {
        if self.mode == Mode::Unsupported {
            return Err("Unsupported connection mode");
        }
        let input = self.address.trim();
        if input.contains('@') {
            return Err("Proxy authentication is not supported");
        }
        let authority = input
            .strip_prefix("http://")
            .ok_or("Enter an HTTP proxy URL, for example http://127.0.0.1:8080")?;
        let authority = authority.strip_suffix('/').unwrap_or(authority);
        if authority
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
            || authority.contains(['/', '?', '#', '\\'])
        {
            return Err("Use a proxy host and port without path, query or fragment");
        }
        let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
            let (host, port) = rest.split_once("]:").ok_or("Use [IPv6]:port")?;
            if !matches!(host.parse::<IpAddr>(), Ok(IpAddr::V6(_))) {
                return Err("Invalid IPv6 proxy address");
            }
            (host, port)
        } else {
            let (host, port) = authority
                .rsplit_once(':')
                .ok_or("Enter an explicit proxy port")?;
            if !dns(host) {
                return Err("Invalid proxy hostname");
            }
            (host, port)
        };
        let port = port
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or("Proxy port must be between 1 and 65535")?;
        Ok(Proxy {
            host: host.to_ascii_lowercase(),
            port,
            bypass: rules(&self.bypass)?,
        })
    }
}
fn dns(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.trim_end_matches('.').split('.').all(|s| {
            !s.is_empty()
                && s.len() <= 63
                && !s.starts_with('-')
                && !s.ends_with('-')
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}
fn rules(value: &str) -> Result<Vec<String>, &'static str> {
    let mut result = vec![
        "localhost".into(),
        ".localhost".into(),
        "127.0.0.1".into(),
        "::1".into(),
    ];
    if !value.trim().is_empty() {
        for item in value.split(',') {
            let item = item.trim().trim_end_matches('.').to_ascii_lowercase();
            if item.starts_with("..") {
                return Err("Invalid bypass domain");
            }
            let host = item.trim_start_matches('.');
            if host.parse::<IpAddr>().is_err() && !dns(host) {
                return Err(
                    "Bypass accepts comma-separated hosts, domains or IP addresses (no URLs, ports or CIDR)",
                );
            }
            result.push(
                host.parse::<IpAddr>()
                    .map(|ip| ip.to_string())
                    .unwrap_or_else(|_| host.to_owned()),
            );
            if host.parse::<IpAddr>().is_err() {
                result.push(format!(".{host}"));
            }
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}
fn matches_host(rules: &[String], host: &str) -> bool {
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if let Ok(ip) = host.parse::<IpAddr>() {
        return rules.iter().any(|rule| {
            rule.parse::<IpAddr>()
                .is_ok_and(|candidate| candidate == ip)
        });
    }
    rules
        .iter()
        .any(|rule| host == *rule || (rule.starts_with('.') && host.ends_with(rule)))
}
/// Match the built-in local set and additional validated domain boundaries.
pub fn bypasses(rules_text: &str, host: &str) -> bool {
    rules(rules_text).is_ok_and(|rules| matches_host(&rules, host))
}

/// Read malformed/future network data without discarding unrelated preferences.
pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Network, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or(Network {
        mode: Mode::Unsupported,
        ..Default::default()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn per_harness_routes_migrate_and_isolate_environment() {
        let mut n: Network = serde_json::from_str("{}").unwrap();
        for h in crate::HarnessKind::ALL {
            assert_eq!(n.agent_route(h), AgentRoute::Follow);
            assert!(n.agent_environment(h).unwrap().is_empty());
        }
        n.address = "http://127.0.0.1:8080".into();
        n.scopes[Scope::Agent as usize] = false;
        for h in crate::HarnessKind::ALL {
            n.agents.insert(h.program().into(), AgentRoute::Proxy);
            let env = n.agent_environment(h).unwrap();
            assert_eq!(env.iter().filter(|(_, v)| v.is_some()).count(), 6);
            assert_eq!(
                env.iter()
                    .find(|(k, _)| *k == "HTTPS_PROXY")
                    .unwrap()
                    .1
                    .as_deref(),
                Some("http://127.0.0.1:8080")
            );
            n.agents.insert(h.program().into(), AgentRoute::Direct);
            n.address.clear();
            n.mode = Mode::Custom;
            let env = n.agent_environment(h).unwrap();
            assert_eq!(env.len(), 8);
            assert!(env.iter().all(|(_, v)| v.is_none()));
            n.agents.insert(h.program().into(), AgentRoute::Proxy);
            assert!(n.agent_environment(h).is_err());
            n.address = "http://127.0.0.1:8080".into();
            n.mode = Mode::System;
        }
        n.agents.insert("future-harness".into(), AgentRoute::Direct);
        let restored: Network = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
        assert_eq!(restored, n);
        assert!(n.policy(Scope::Guild).unwrap().is_none());
    }
    #[test]
    fn numeric_ipv6_bypass_equivalence() {
        assert!(bypasses("2001:0db8:0:0:0:0:0:7", "[2001:db8::7]"));
        assert!(bypasses("", "[0:0:0:0:0:0:0:1]"));
        assert!(!bypasses("2001:db8::7", "2001:db8::8"));
    }
    #[test]
    fn local_and_domain_boundaries() {
        for host in [
            "localhost",
            "a.localhost",
            "127.0.0.1",
            "::1",
            "internal.example",
            "api.INTERNAL.example.",
        ] {
            assert!(bypasses(".internal.example", host), "{host}");
        }
        for host in ["badinternal.example", "internal.example.evil", "10.0.0.1"] {
            assert!(!bypasses(".internal.example", host));
        }
    }
    #[test]
    fn migration_drafts_independence_and_redaction() {
        let malformed: crate::Settings =
            serde_json::from_str(r#"{"network":{"mode":17},"copy_on_select":true}"#).unwrap();
        assert!(malformed.copy_on_select);
        assert_eq!(malformed.network.mode, Mode::Unsupported);
        let settings: crate::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.network, Network::default());
        let mut n = Network {
            mode: Mode::Custom,
            address: "http://127.0.0.1:8080".into(),
            ..Default::default()
        };
        assert_eq!(n.custom().unwrap().url(), n.address);
        n.scopes[Scope::Agent as usize] = false;
        assert!(n.policy(Scope::Agent).unwrap().is_none());
        assert!(n.policy(Scope::Guild).unwrap().is_some());
        n.mode = Mode::System;
        assert!(n.policy(Scope::Guild).unwrap().is_none());
        assert_eq!(n.address, "http://127.0.0.1:8080");
        n.address = "http://fake:SECRET@proxy:8080".into();
        assert!(!format!("{n:?}").contains("SECRET"));
        assert!(!serde_json::to_string(&n).unwrap().contains("SECRET"));
        assert!(n.custom().is_err());
    }
    #[test]
    fn validation_and_explicit_environment_removal() {
        let mut n = Network {
            mode: Mode::Custom,
            ..Default::default()
        };
        for s in [
            "",
            "https://proxy:80",
            "http://proxy:0",
            "http://proxy:99999",
            "http://proxy:80/path",
            "http://a@proxy:80",
            "http://proxy:80?x",
        ] {
            n.address = s.into();
            assert!(n.custom().is_err(), "{s}");
        }
        n.address = "http://[::1]:8080".into();
        let p = n.custom().unwrap();
        assert_eq!(
            p.environment().iter().filter(|(_, v)| v.is_none()).count(),
            2
        );
        assert_eq!(p.url(), n.address);
        n.bypass = "http://internal".into();
        assert!(n.custom().is_err());
    }
}
