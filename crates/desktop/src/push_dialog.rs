//! Push Commits as a window — M8.3.6. Pure: what the window offers and
//! which commit it shows, from what git last said.

/// Whether *Push* can be pressed: something is waiting, there is somewhere
/// to send it, and no push is out already — a second one queued behind the
/// first is two pushes for one press.
pub fn can_push(waiting: usize, has_destination: bool, running: bool) -> bool {
    waiting > 0 && has_destination && !running
}

/// Commit selection, separate from the branch's Push action. Selecting
/// fewer commits changes inspection, never what the branch sends.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub shas: Vec<String>,
    anchor: Option<String>,
}

impl Selection {
    /// A plain click selects one; Command/Control toggles; Shift spans
    /// from the last plain click. The list is newest first.
    pub fn click(&mut self, sha: &str, shas: &[&str], toggle: bool, extend: bool) {
        let Some(index) = shas.iter().position(|s| *s == sha) else {
            return;
        };
        if extend
            && let Some(anchor) = self
                .anchor
                .as_deref()
                .and_then(|a| shas.iter().position(|s| *s == a))
        {
            self.shas = shas[index.min(anchor)..=index.max(anchor)]
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
        } else if toggle {
            if self.shas.iter().any(|s| s == sha) {
                self.shas.retain(|s| s != sha);
            } else {
                self.shas.push(sha.to_owned());
            }
            self.anchor = Some(sha.to_owned());
        } else {
            self.shas = vec![sha.to_owned()];
            self.anchor = Some(sha.to_owned());
        }
    }

    pub fn retain(&mut self, shas: &[&str]) {
        self.shas.retain(|sha| shas.contains(&sha.as_str()));
        if self
            .anchor
            .as_ref()
            .is_some_and(|sha| !shas.contains(&sha.as_str()))
        {
            self.anchor = None;
        }
    }
}

/// The revisions inspected: selected hashes in log order, as the key
/// for git::inspection's combined changes.
/// An empty selection inspects the whole branch. Both ends are immutable
/// hashes from the list, so a moving HEAD cannot change an open diff.
pub fn comparison(selected: &[String], shas: &[&str]) -> Option<String> {
    let shown: Vec<&str> = shas
        .iter()
        .copied()
        .filter(|s| selected.is_empty() || selected.iter().any(|chosen| chosen == s))
        .collect();
    match shown.as_slice() {
        [] => None,
        [one] => Some((*one).to_owned()),
        _ => Some(shown.join(",")),
    }
}

/// A remote branch name, never a refspec, flag or revision expression.
/// Git checks the full ref again on the worker before reading its range.
pub fn valid_target(name: &str) -> bool {
    !name.is_empty()
        && name != "@"
        && !name.starts_with(['-', '+'])
        && !name.ends_with('.')
        && !name.contains("..")
        && !name.contains("@{")
        && !name.chars().any(|c| c.is_control() || c.is_whitespace())
        && !name.contains(['~', '^', ':', '?', '*', '[', '\\'])
        && name
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}

/// What Push Commits says when git refuses: git's own lines without its
/// `hint:`s and without the `To <remote>` the head already names.
pub fn refusal(words: &str) -> String {
    words
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("hint:") && !line.starts_with("To "))
        .collect::<Vec<_>>()
        .join("\n")
}
