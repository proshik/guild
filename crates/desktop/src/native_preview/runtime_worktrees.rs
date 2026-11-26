//! Marked, provider-free production Root cadence driver; no alternate read cache.
use crate::{Root, root::isolation::Policy};
use gpui::{App, AppContext, Entity};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Instant,
};

pub fn root(fixture: &Path, count: usize, cx: &mut App) -> Entity<Root> {
    assert!(fixture.is_absolute() && fixture.join(".owned-runtime-worktrees").is_file());
    assert!((1..=100).contains(&count));
    let mut desk = guild_session::Desk::default();
    desk.normalise();
    for i in 0..count {
        let path = fixture.join(format!("root-{i}"));
        assert!(path.is_dir());
        desk.open_in(desk.active_workspace(), &path);
    }
    cx.new(|cx| {
        Root::from_state_with_policy(
            guild_session::Settings {
                autosave: false,
                ..Default::default()
            },
            desk,
            Policy::Diagnostic {
                state_dir: crate::paths::state_dir(),
            },
            cx,
        )
    })
}
pub fn poll(root: &Entity<Root>, cx: &mut App) {
    root.update(cx, |r, cx| r.poll_git(cx));
}
pub fn fence(root: &Entity<Root>, cx: &mut App) -> Vec<mpsc::Receiver<()>> {
    root.update(cx, |r, _| {
        r.status_workers
            .values()
            .map(|w| w.diagnostic_fence())
            .collect()
    })
}
pub fn begin(root: &Entity<Root>, cx: &mut App) {
    root.update(cx, |r, _| {
        r.worktrees_asked = Some(Instant::now());
    });
}
pub fn close(root: &Entity<Root>, cx: &mut App) {
    root.update(cx, |r, cx| {
        let ids: Vec<_> = r.desk.projects().map(|p| p.id).collect();
        for id in ids {
            r.desk.remove_project(id);
        }
        r.poll_git(cx);
    });
}
pub fn snapshot(root: &Entity<Root>, cx: &App) -> serde_json::Value {
    let r = root.read(cx);
    serde_json::json!({"roots":r.status_workers.len(),"groups":r.worktrees.len(),"status_ok":r.status_workers.keys().all(|id|r.git_status.get(id).is_some_and(Result::is_ok)),"paths":r.desk.projects().map(|p|p.path.clone()).collect::<Vec<PathBuf>>()})
}

pub fn cadence(root: &Entity<Root>, cx: &mut App) {
    root.update(cx, |r, cx| {
        r.diagnostic_worktree_cadence();
        r.poll_git(cx);
    });
}
