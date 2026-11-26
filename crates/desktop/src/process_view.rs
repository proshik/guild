//! Read-only native Process projection. Selection never advances the run graph.
use guild_session::workflow::{Definition, Step};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct Phase {
    pub step: Step,
    pub state: String,
    pub detail: Option<String>,
}

pub(crate) fn phases(snapshot: &Value) -> Result<(Definition, Vec<Phase>), String> {
    let definition: Definition = serde_json::from_value(snapshot["graph"]["definition"].clone())
        .map_err(|e| e.to_string())?;
    let phases = definition
        .order()?
        .into_iter()
        .map(|step| {
            let node = snapshot["nodes"]
                .as_array()
                .and_then(|nodes| nodes.iter().find(|n| n["step"] == step.id));
            Phase {
                step: step.clone(),
                state: node
                    .and_then(|n| n["state"].as_str())
                    .unwrap_or("unknown")
                    .into(),
                detail: node.and_then(|n| n["detail"].as_str()).map(str::to_owned),
            }
        })
        .collect();
    Ok((definition, phases))
}

pub(crate) fn current(phases: &[Phase]) -> Option<&str> {
    ["active", "waiting", "ready"]
        .into_iter()
        .find_map(|state| {
            phases
                .iter()
                .find(|p| p.state == state)
                .map(|p| p.step.id.as_str())
        })
        .or_else(|| phases.last().map(|p| p.step.id.as_str()))
}

#[derive(Default, Debug)]
pub(crate) struct Selection {
    pub step: Option<String>,
    pub removed: Option<String>,
}
impl Selection {
    pub fn reconcile(&mut self, phases: &[Phase]) {
        if self
            .step
            .as_ref()
            .is_some_and(|id| phases.iter().any(|p| p.step.id == *id))
        {
            return;
        }
        self.removed = self.step.take();
        self.step = current(phases).map(str::to_owned);
    }
    pub fn select(&mut self, step: &str, phases: &[Phase]) -> bool {
        if !phases.iter().any(|p| p.step.id == step) {
            return false;
        }
        self.step = Some(step.to_owned());
        self.removed = None;
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Artifact {
    pub label: String,
    pub path: PathBuf,
    pub sha256: Option<String>,
    pub different_source: bool,
}
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Only explicitly recorded files. Directory/archive references are context,
/// never invented file actions; traversal cannot become a project file link.
pub(crate) fn artifacts(snapshot: &Value, step: &str) -> Vec<Artifact> {
    let mut rows = Vec::new();
    let pinned = &snapshot["accepted_contract"];
    if let Some(directory) = pinned["directory"].as_str() {
        for artifact in pinned["contract"]["artifacts"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(path) = artifact["path"]
                .as_str()
                .map(Path::new)
                .filter(|p| relative(p))
            {
                rows.push(Artifact {
                    label: path.display().to_string(),
                    path: Path::new(directory).join(path),
                    sha256: artifact["sha256"].as_str().map(str::to_owned),
                    different_source: false,
                });
            }
        }
    }
    let checked = snapshot["graph"]["history"]
        .as_array()
        .into_iter()
        .flatten();
    let reported = snapshot["reports"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|report| report["qualified"].as_array().into_iter().flatten());
    for outcome in checked.chain(reported).filter(|o| o["step"] == step) {
        let evidence = &outcome["evidence"];
        for artifact in evidence["artifacts"].as_array().into_iter().flatten() {
            if let Some(path) = artifact["path"]
                .as_str()
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
            {
                rows.push(Artifact {
                    label: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    path,
                    sha256: artifact["sha256"].as_str().map(str::to_owned),
                    different_source: evidence["source"]["sha256"] != snapshot["source"]["sha256"],
                });
            }
        }
    }
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    rows.dedup_by(|a, b| a.path == b.path && a.sha256 == b.sha256);
    rows
}

pub(crate) fn project_file(path: &Path, root: &Path) -> Option<PathBuf> {
    let path = path.strip_prefix(root).ok()?;
    relative(path).then(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Value {
        let mut definition = Definition::shipped().unwrap();
        definition.steps.reverse();
        definition
            .steps
            .iter_mut()
            .for_each(|s| s.label = format!("Custom / Пользовательский {}", s.id));
        let graph = guild_session::workflow::RunGraph::new(definition.clone(), false).unwrap();
        serde_json::json!({"graph":{"definition":definition},"nodes":graph.nodes()})
    }
    #[test]
    fn selection_is_independent_of_execution_and_survives_refresh() {
        let mut snapshot = snapshot();
        let (_, phases) = phases(&snapshot).unwrap();
        let mut selection = Selection::default();
        selection.reconcile(&phases);
        let first = phases[0].step.id.clone();
        let last = phases.last().unwrap().step.id.clone();
        assert!(selection.select(&last, &phases));
        snapshot["nodes"][0]["state"] = "active".into();
        let (_, phases) = super::phases(&snapshot).unwrap();
        selection.reconcile(&phases);
        assert_eq!(current(&phases), Some(first.as_str()));
        assert_eq!(
            selection.step.as_deref(),
            Some(last.as_str()),
            "refresh must not select the executing phase instead of the inspected phase"
        );
        assert!(!selection.select("foreign-step", &phases));
        assert_eq!(selection.step.as_deref(), Some(last.as_str()));
    }
    #[test]
    fn all_custom_steps_remain_in_topological_order_when_node_telemetry_is_missing() {
        let mut snapshot = snapshot();
        snapshot["nodes"] = serde_json::json!([]);
        let (definition, phases) = phases(&snapshot).unwrap();
        assert_eq!(phases.len(), definition.steps.len());
        assert_eq!(
            phases.iter().map(|p| &p.step.id).collect::<Vec<_>>(),
            definition
                .order()
                .unwrap()
                .iter()
                .map(|s| &s.id)
                .collect::<Vec<_>>()
        );
        assert!(
            phases
                .iter()
                .all(|p| p.state == "unknown" && p.step.label.starts_with("Custom /"))
        );
    }
    #[test]
    fn missing_selection_falls_back_explicitly_and_file_links_refuse_traversal() {
        let (_, phases) = phases(&snapshot()).unwrap();
        let mut selection = Selection {
            step: Some("removed".into()),
            removed: None,
        };
        selection.reconcile(&phases);
        assert_eq!(selection.removed.as_deref(), Some("removed"));
        assert_eq!(selection.step.as_deref(), current(&phases));
        assert_eq!(
            project_file(
                Path::new("/project/.guild/run/evidence.txt"),
                Path::new("/project")
            ),
            Some(PathBuf::from(".guild/run/evidence.txt"))
        );
        for path in ["/project/../secret", "/foreign/report", "/project"] {
            assert!(project_file(Path::new(path), Path::new("/project")).is_none());
        }
        let snapshot = serde_json::json!({"accepted_contract":{"directory":"/project/run/pinned","contract":{"artifacts":[{"path":"../secret"},{"path":"/etc/file"},{"path":"proposal.md","sha256":"pin"}]}}});
        let rows = artifacts(&snapshot, "selected");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].path,
            PathBuf::from("/project/run/pinned/proposal.md")
        );
    }
}
