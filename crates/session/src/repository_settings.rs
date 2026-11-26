//! Preferences shared by every local checkout of one repository.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// A repository override for one supported language service.
pub enum LspPolicy {
    #[default]
    /// Use the global default.
    Inherit,
    /// Permit lazy launches in this repository.
    Enabled,
    /// Prevent launches and stop owned running services.
    Disabled,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
/// Preferences keyed by the canonical common Git directory or non-Git root.
pub struct RepositorySettings {
    /// Language ids to explicit policies; unknown ids survive round trips.
    pub lsp: BTreeMap<String, LspPolicy>,
    /// Optional base for future worktrees.
    pub worktree_base: Option<String>,
    /// Optional parent directory for future worktrees.
    pub worktree_directory: Option<PathBuf>,
    /// Optional installed Lucide icon name for this repository.
    pub icon: Option<String>,
}
impl RepositorySettings {
    /// The override, with an absent entry inheriting.
    pub fn lsp_policy(&self, language: &str) -> LspPolicy {
        self.lsp.get(language).copied().unwrap_or_default()
    }
}
impl crate::Settings {
    /// Whether this repository permits the service, defaulting to enabled.
    pub fn lsp_enabled(&self, repository: &Path, language: &str) -> bool {
        match self
            .repository_settings
            .get(repository)
            .map(|r| r.lsp_policy(language))
            .unwrap_or_default()
        {
            LspPolicy::Enabled => true,
            LspPolicy::Disabled => false,
            LspPolicy::Inherit => self.lsp_defaults.get(language).copied().unwrap_or(true),
        }
    }
}
