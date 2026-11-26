//! Selection shared by file lists. Sources may qualify paths; ranges use visible order.

use std::borrow::Borrow;
use std::path::{Path, PathBuf};

/// Clipboard spelling of a project-relative identity, shared by lists and menus.
pub fn relative_path_text(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        path.display().to_string()
    }
}

pub type FileSelection = Selection<PathBuf>;

#[derive(Debug, Clone)]
pub struct Selection<T> {
    pub selected: Vec<T>,
    pub focus: Option<T>,
    anchor: Option<T>,
}

impl<T> Default for Selection<T> {
    fn default() -> Self {
        Self {
            selected: Vec::new(),
            focus: None,
            anchor: None,
        }
    }
}

impl<T: Clone + Eq> Selection<T> {
    pub fn contains<Q: Eq + ?Sized>(&self, path: &Q) -> bool
    where
        T: Borrow<Q>,
    {
        self.selected.iter().any(|p| p.borrow() == path)
    }

    /// A context click preserves the group when it already contains the target.
    pub fn context_click(&mut self, path: &T, rows: &[T]) {
        if !self.contains(path) {
            self.click(path, rows, false, false);
        }
        if self.contains(path) {
            self.focus = Some(path.clone());
        }
    }

    pub fn click<Q: Eq + ?Sized>(&mut self, path: &Q, rows: &[T], toggle: bool, extend: bool)
    where
        T: Borrow<Q>,
    {
        let Some(index) = rows.iter().position(|p| p.borrow() == path) else {
            return;
        };
        self.retain(rows);
        let anchor = self
            .anchor
            .as_ref()
            .and_then(|p| rows.iter().position(|r| r == p));
        if extend {
            let anchor = anchor.unwrap_or(index);
            let range = &rows[anchor.min(index)..=anchor.max(index)];
            if !toggle {
                self.selected.clear();
            }
            for path in range {
                if !self.selected.contains(path) {
                    self.selected.push(path.clone());
                }
            }
            if self.anchor.is_none() {
                self.anchor = Some(rows[index].clone());
            }
        } else {
            if toggle {
                if self.contains(path) {
                    self.selected.retain(|p| p.borrow() != path);
                } else {
                    self.selected.push(rows[index].clone());
                }
            } else {
                self.selected = vec![rows[index].clone()];
            }
            self.anchor = Some(rows[index].clone());
        }
        self.focus = Some(rows[index].clone());
        self.selected
            .sort_by_key(|p| rows.iter().position(|r| r == p));
    }

    /// The arrows, Home and End move the selection, with Command or
    /// without — decision 54: no cursor walks apart from it, so there is
    /// no Command-Space to toggle one. Shift extends a range. Command-A
    /// selects visible files only.
    pub fn key(&mut self, key: &str, rows: &[T], command: bool, shift: bool) -> bool {
        self.retain(rows);
        if command && key == "a" {
            self.selected = rows.to_vec();
            if self.focus.is_none() {
                self.focus = rows.first().cloned();
            }
            if self.anchor.is_none() {
                self.anchor = self.focus.clone();
            }
            return true;
        }
        let current = self
            .focus
            .as_ref()
            .and_then(|p| rows.iter().position(|r| r == p));
        let next = match key {
            "down" | "up" => crate::pick::step(rows.len(), current, key == "down"),
            "home" => (!rows.is_empty()).then_some(0),
            "end" => rows.len().checked_sub(1),
            _ => return false,
        };
        if let Some(index) = next {
            self.click(&rows[index], rows, false, shift);
        }
        true
    }

    /// Map identities without changing selection order, focus or range anchor.
    pub(crate) fn remap(&mut self, mut map: impl FnMut(&mut T)) {
        for value in &mut self.selected {
            map(value);
        }
        if let Some(value) = &mut self.focus {
            map(value);
        }
        if let Some(value) = &mut self.anchor {
            map(value);
        }
    }

    pub fn retain(&mut self, rows: &[T]) {
        self.selected.retain(|p| rows.contains(p));
        if self.focus.as_ref().is_some_and(|p| !rows.contains(p)) {
            self.focus = None;
        }
        if self.anchor.as_ref().is_some_and(|p| !rows.contains(p)) {
            self.anchor = None;
        }
    }

    /// Only a highlighted cursor may be used by a single-file action.
    pub fn action_target(&self) -> Option<&T> {
        self.focus.as_ref().filter(|path| self.contains(*path))
    }
}

impl FileSelection {
    /// A partial batch completion removes only successful sources and their children.
    /// Failed sources retain their selection; a removed cursor cannot retarget them.
    pub fn remove_completed(&mut self, completed: &[PathBuf]) {
        let remains = |path: &PathBuf| !completed.iter().any(|done| path.starts_with(done));
        self.selected.retain(remains);
        if self.focus.as_ref().is_some_and(|p| !remains(p)) {
            self.focus = None;
        }
        if self.anchor.as_ref().is_some_and(|p| !remains(p)) {
            self.anchor = None;
        }
    }

    pub fn copied_paths(&self) -> String {
        self.selected
            .iter()
            .map(|p| relative_path_text(p))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
