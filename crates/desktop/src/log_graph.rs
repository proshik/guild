//! Commit ancestry laid out once per loaded history, independent of painting.
use crate::git::log::Commit;

#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub rows: Vec<Row>,
    pub lanes: usize,
}
#[derive(Clone, Debug, Default)]
/// One node and the edge segments crossing its row.
pub struct Row {
    pub lane: usize,
    pub color: usize,
    pub edges: Vec<Edge>,
}
#[derive(Clone, Debug, PartialEq)]
/// Lane endpoints and normalized vertical positions (top 0, node 0.5, bottom 1).
/// A segment ending at 0.85 is an unresolved parent stub, not a continuation.
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub start: f32,
    pub end: f32,
    pub color: usize,
}
impl Graph {
    /// Input is git's `--topo-order`: descendants precede their parents.
    /// Missing parents get short terminal stubs, never an edge to the next row.
    pub fn of(commits: &[Commit]) -> Self {
        use std::collections::{HashMap, HashSet};
        let positions: HashMap<&str, usize> = commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.sha.as_str(), i))
            .collect();
        let mut active: Vec<Option<(String, usize)>> = Vec::new();
        let mut next_color = 0;
        let mut graph = Self::default();
        for (index, commit) in commits.iter().enumerate() {
            let incoming = active
                .iter()
                .position(|v| v.as_ref().is_some_and(|(sha, _)| sha == &commit.sha));
            let lane = incoming.unwrap_or_else(|| vacant(&mut active));
            let color = incoming
                .and_then(|i| active[i].as_ref().map(|(_, c)| *c))
                .unwrap_or_else(|| {
                    let c = next_color;
                    next_color += 1;
                    c
                });
            let mut row = Row {
                lane,
                color,
                edges: Vec::new(),
            };
            for (i, entry) in active.iter().enumerate() {
                if let Some((_, color)) = entry {
                    row.edges.push(Edge {
                        from: i,
                        to: i,
                        start: 0.0,
                        end: if i == lane { 0.5 } else { 1.0 },
                        color: *color,
                    });
                }
            }
            active[lane] = None;
            let mut parents = HashSet::new();
            let mut temporary = Vec::new();
            for (parent_index, parent) in commit
                .parents
                .iter()
                .filter(|p| parents.insert(p.as_str()))
                .enumerate()
            {
                let existing = active
                    .iter()
                    .position(|v| v.as_ref().is_some_and(|(sha, _)| sha == parent));
                let to = existing.unwrap_or_else(|| vacant(&mut active));
                let edge_color = existing
                    .and_then(|i| active[i].as_ref().map(|(_, c)| *c))
                    .unwrap_or_else(|| {
                        if parent_index == 0 {
                            color
                        } else {
                            let c = next_color;
                            next_color += 1;
                            c
                        }
                    });
                let loaded = positions.get(parent.as_str()).is_some_and(|p| *p > index);
                row.edges.push(Edge {
                    from: lane,
                    to,
                    start: 0.5,
                    end: if loaded { 1.0 } else { 0.85 },
                    color: edge_color,
                });
                active[to] = Some((parent.clone(), edge_color));
                if !loaded {
                    temporary.push(to);
                }
            }
            graph.lanes = graph.lanes.max(active.len()).max(lane + 1);
            for i in temporary {
                active[i] = None;
            }
            while active.last().is_some_and(Option::is_none) {
                active.pop();
            }
            graph.rows.push(row);
        }
        graph
    }
}

fn vacant(active: &mut Vec<Option<(String, usize)>>) -> usize {
    if let Some(i) = active.iter().position(Option::is_none) {
        i
    } else {
        active.push(None);
        active.len() - 1
    }
}
