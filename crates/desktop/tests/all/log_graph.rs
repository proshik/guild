use guild_desktop::{git::log::Commit, log_graph::Graph};
fn commits(rows: &[(&str, &[&str])]) -> Vec<Commit> {
    rows.iter()
        .map(|(sha, parents)| Commit {
            sha: sha.to_string(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            author: String::new(),
            email: String::new(),
            time: 0,
            refs: vec![],
            summary: String::new(),
            body: String::new(),
        })
        .collect()
}
#[test]
fn linear_history_has_connected_parent_edges() {
    let graph = Graph::of(&commits(&[("a", &["b"]), ("b", &["c"]), ("c", &[])]));
    assert_eq!(graph.rows.len(), 3);
    assert_eq!(graph.lanes, 1);
    assert!(
        graph.rows[0]
            .edges
            .iter()
            .any(|e| e.start == 0.5 && e.end == 1.0)
    );
    assert!(
        graph.rows[1]
            .edges
            .iter()
            .any(|e| e.start == 0.0 && e.end == 0.5)
    );
    assert!(graph.rows[2].edges.iter().all(|e| e.end <= 0.5));
}

#[test]
fn branches_merge_and_octopus_keep_distinct_lanes_until_their_parent() {
    let graph = Graph::of(&commits(&[
        ("merge", &["left", "right", "third"]),
        ("left", &["base"]),
        ("right", &["base"]),
        ("third", &["base"]),
        ("base", &[]),
    ]));
    assert_eq!(graph.lanes, 3);
    let targets: Vec<_> = graph.rows[0].edges.iter().map(|e| e.to).collect();
    assert_eq!(targets, [0, 1, 2]);
    assert_eq!(
        graph.rows.iter().map(|r| r.lane).collect::<Vec<_>>(),
        [0, 0, 1, 2, 0]
    );
    assert_eq!(
        graph.rows[2]
            .edges
            .iter()
            .find(|e| e.start == 0.5)
            .unwrap()
            .to,
        0
    );
    assert_eq!(
        graph.rows[3]
            .edges
            .iter()
            .find(|e| e.start == 0.5)
            .unwrap()
            .to,
        0
    );
    // At every row boundary, a lane has exactly the same color on either side.
    for pair in graph.rows.windows(2) {
        let mut bottom: Vec<_> = pair[0]
            .edges
            .iter()
            .filter(|e| e.end == 1.0)
            .map(|e| (e.to, e.color))
            .collect();
        bottom.sort();
        bottom.dedup();
        let mut top: Vec<_> = pair[1]
            .edges
            .iter()
            .filter(|e| e.start == 0.0)
            .map(|e| (e.from, e.color))
            .collect();
        top.sort();
        top.dedup();
        assert_eq!(bottom, top);
    }
}

#[test]
fn omitted_parents_are_stubs_not_edges_to_filtered_neighbors() {
    let graph = Graph::of(&commits(&[
        ("a", &["hidden", "also-hidden"]),
        ("unrelated", &[]),
    ]));
    assert_eq!(graph.rows[0].edges.len(), 2);
    assert!(graph.rows[0].edges.iter().all(|e| e.end < 1.0));
    assert!(graph.rows[1].edges.is_empty());
    assert_ne!(graph.rows[0].color, graph.rows[1].color);
}

#[test]
fn loading_the_next_page_extends_only_the_actual_parent() {
    let first = Graph::of(&commits(&[("a", &["parent"])]));
    let next = Graph::of(&commits(&[
        ("a", &["parent"]),
        ("unrelated", &[]),
        ("parent", &[]),
    ]));
    assert!(first.rows[0].edges[0].end < 1.0);
    assert_eq!(next.rows[0].edges[0].end, 1.0);
    assert_ne!(next.rows[1].lane, next.rows[2].lane);
    assert!(
        next.rows[1]
            .edges
            .iter()
            .all(|e| e.start == 0.0 && e.end == 1.0)
    );
    assert_eq!(next.rows[0].color, next.rows[2].color);
}

#[test]
fn empty_history_and_disconnected_roots_have_no_edges() {
    assert!(Graph::of(&[]).rows.is_empty());
    let graph = Graph::of(&commits(&[("one", &[]), ("two", &[])]));
    assert_eq!(graph.lanes, 1);
    assert!(graph.rows.iter().all(|r| r.edges.is_empty()));
}

#[test]
fn log_requests_topological_order() {
    use guild_desktop::git::{Filter, Scope, log_args};
    assert!(log_args(&Scope::All, &Filter::default(), 0, 200).contains(&"--topo-order".to_owned()));
}

#[test]
fn every_drawn_route_ends_at_its_actual_parent_across_many_forks() {
    for seed in 0..64 {
        let commits: Vec<Commit> = (0..24)
            .map(|i| Commit {
                sha: i.to_string(),
                parents: ((i + 1)..24)
                    .filter(|j| (i * 31 + j * 17 + seed * 7) % 11 < 2)
                    .map(|j| j.to_string())
                    .collect(),
                author: String::new(),
                email: String::new(),
                time: 0,
                refs: vec![],
                summary: String::new(),
                body: String::new(),
            })
            .collect();
        let graph = Graph::of(&commits);
        for (i, row) in graph.rows.iter().enumerate() {
            let mut reached = Vec::new();
            for edge in row.edges.iter().filter(|e| e.start == 0.5) {
                let mut lane = edge.to;
                for (j, next) in graph.rows.iter().enumerate().skip(i + 1) {
                    if next.lane == lane {
                        reached.push(commits[j].sha.clone());
                        break;
                    }
                    let continuation = next
                        .edges
                        .iter()
                        .find(|e| e.from == lane && e.start == 0.0)
                        .expect("a drawn route cannot vanish between rows");
                    assert_eq!(continuation.end, 1.0);
                    lane = continuation.to;
                }
            }
            reached.sort();
            let mut parents = commits[i].parents.clone();
            parents.sort();
            assert_eq!(reached, parents, "seed {seed}, commit {i}");
        }
    }
}
