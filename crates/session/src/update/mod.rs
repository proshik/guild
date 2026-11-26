//! Shared application update vocabulary and persistence, independent of the window.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub mod lock;
mod preferences;
pub use preferences::{PreferenceState, PreferenceStore};
/// User-selected update transport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A completed local build.
    Local,
    /// A published remote release.
    #[default]
    Remote,
}
impl Source {
    /// Stable machine spelling, independent of the current Remote provider.
    pub fn name(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }
}
/// Remote release channel, retained while Local is selected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Published nightly prereleases.
    #[default]
    Nightly,
    /// Published stable releases.
    Stable,
}
impl Channel {
    /// Stable machine spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Nightly => "nightly",
            Self::Stable => "stable",
        }
    }
    /// Whether GitHub's classification and exact tag agree with this channel.
    pub fn accepts(self, tag: &str, prerelease: bool, draft: bool) -> bool {
        !draft
            && match self {
                Self::Nightly => {
                    prerelease && tag.strip_prefix("nightly-").is_some_and(|s| !s.is_empty())
                }
                Self::Stable => {
                    !prerelease
                        && tag.strip_prefix('v').is_some_and(|v| {
                            let p: Vec<_> = v.split('.').collect();
                            p.len() == 3
                                && p.iter().all(|s| {
                                    !s.is_empty()
                                        && (s.len() == 1 || !s.starts_with('0'))
                                        && s.bytes().all(|c| c.is_ascii_digit())
                                        && s.parse::<u64>().is_ok()
                                })
                        })
                }
            }
    }
}
/// Choices owned by the updater rather than legacy Settings snapshots.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Source to check.
    pub source: Source,
    /// Saved even while Local is selected.
    pub channel: Channel,
    /// Absolute directory containing completed builds; None uses state/update.
    pub local_directory: Option<PathBuf>,
}

mod protocol;
pub use protocol::*;

/// Headless validation shared by release tooling and the desktop installer.
pub mod bundle;

/// Bounded archive inspection before macOS resource-preserving extraction.
pub mod archive;

/// Explicit owner-side offline release preparation and verification.
pub mod publisher;

mod identity;
pub use identity::{Artifact, Identity, Origin};
/// Durable replacement transactions; all mutation remains outside the UI thread.
pub mod transaction;
