//! Persisted Open In composition; installed application discovery belongs to desktop.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf};

/// A user-connected application, identified independently of its editable name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomApp {
    /// Stable row identity.
    pub id: String,
    /// Editable display name.
    pub name: String,
    /// Exact chosen installed bundle, not a command line.
    pub path: Option<PathBuf>,
    /// Validated bundle identity at selection time.
    pub bundle_id: Option<String>,
}
/// User order and visibility overrides; absent settings preserve existing defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Stable builtin/custom row keys in the person's order.
    pub order: Vec<String>,
    /// Explicit hidden rows.
    pub disabled: BTreeSet<String>,
    /// Additional application connections.
    pub custom: Vec<CustomApp>,
}
impl Config {
    /// Known rows, preserving saved order and appending newly introduced defaults.
    pub fn keys(&self, defaults: &[&str]) -> Vec<String> {
        let known: BTreeSet<&str> = defaults
            .iter()
            .copied()
            .chain(self.custom.iter().map(|row| row.id.as_str()))
            .collect();
        let mut seen = BTreeSet::new();
        self.order
            .iter()
            .map(String::as_str)
            .chain(defaults.iter().copied())
            .chain(self.custom.iter().map(|row| row.id.as_str()))
            .filter(|id| known.contains(id) && seen.insert((*id).to_owned()))
            .map(str::to_owned)
            .collect()
    }
    /// Whether a configured row is enabled.
    pub fn enabled(&self, id: &str) -> bool {
        !self.disabled.contains(id)
    }
    /// Set visibility without changing order or the selected application.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        if enabled {
            self.disabled.remove(id);
        } else {
            self.disabled.insert(id.to_owned());
        }
    }
    /// Move one known row one place, keeping unknown future keys in the file.
    pub fn move_item(&mut self, id: &str, down: bool, defaults: &[&str]) {
        let mut keys = self.keys(defaults);
        if let Some(at) = keys.iter().position(|key| key == id) {
            let next = if down {
                (at + 1).min(keys.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            keys.swap(at, next);
            keys.extend(
                self.order
                    .iter()
                    .filter(|key| !keys.contains(key))
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            self.order = keys;
        }
    }
    /// Add an initially unconnected custom row.
    pub fn add_custom(&mut self) -> String {
        let id = format!("custom:{}", crate::SessionId::new());
        self.custom.push(CustomApp {
            id: id.clone(),
            ..Default::default()
        });
        id
    }
    /// Connect a chosen installed bundle, retaining an existing edited name.
    pub fn connect(&mut self, id: &str, path: PathBuf, bundle_id: String, name: &str) {
        if let Some(row) = self.custom.iter_mut().find(|row| row.id == id) {
            row.path = Some(path);
            row.bundle_id = Some(bundle_id);
            if row.name.trim().is_empty() {
                row.name = name.to_owned();
            }
        }
    }
    /// Rename a custom row.
    pub fn rename(&mut self, id: &str, name: String) {
        if let Some(row) = self.custom.iter_mut().find(|row| row.id == id) {
            row.name = name;
        }
    }
    /// Remove only a custom connection and its overrides.
    pub fn remove_custom(&mut self, id: &str) {
        if self.custom.iter().any(|row| row.id == id) {
            self.custom.retain(|row| row.id != id);
            self.order.retain(|key| key != id);
            self.disabled.remove(id);
        }
    }
}
