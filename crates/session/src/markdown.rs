//! A Markdown pane's presentation, independent of its file buffer.
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The three mutually exclusive views of one Markdown buffer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Editable source only.
    #[default]
    Editor,
    /// Source on the left and rendered content on the right.
    Split,
    /// Rendered content only.
    Preview,
}

/// Markdown suffixes are ASCII case-insensitive; no content guessing is needed.
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// A source file with the shared Source/Split/Preview presentation.
pub fn has_preview(path: &Path) -> bool {
    is_markdown(path) || diagram_engine(path).is_some()
}

/// Official local diagram engine for a standalone source suffix.
pub fn diagram_engine(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "mmd" | "mermaid" => Some("mermaid"),
        "puml" | "plantuml" => Some("plantuml"),
        _ => None,
    }
}
