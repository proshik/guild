//! A large column with merge requests drawn, for timing the frames a landed
//! MR answer costs in a native window — GLD-063. No glab and no git: the
//! worktree lists and the answers are the fixture's, landed the way
//! `poll_forge` lands one, and the Root runs no background service.
use crate::{
    Root,
    forge::{Answer, Ask, Batch, Candidate, Resolution, Trigger},
    root::isolation::Policy,
};
use gpui::{App, AppContext, Entity};
use guild_session::{
    Desk, Runs, SessionMode, SessionRecord,
    merge_requests::status::{MergeRequest, State},
};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

/// How a landed answer asks for a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notify {
    /// Every answer notifies the root — `poll_forge` before GLD-063's
    /// notify fix.
    Always,
    /// Only an answer that changed what is drawn — `poll_forge` now.
    IfChanged,
}

fn repository(dir: &Path, r: usize) -> PathBuf {
    dir.join(format!("repository-{r}"))
}

/// The production Root over `repos` repositories of `worktrees` opened
/// worktrees and two unopened ones, `tabs` session tabs on each opened one,
/// under the diagnostic policy and the state directory in force.
pub fn root(dir: &Path, repos: usize, worktrees: usize, tabs: usize, cx: &mut App) -> Entity<Root> {
    let mut desk = Desk::default();
    desk.normalise();
    let workspace = desk.active_workspace();
    let mut lists = Vec::new();
    let mut seats = Vec::new();
    for r in 0..repos {
        let main = repository(dir, r);
        let common = main.join(".git");
        let mut porcelain = String::new();
        for w in 0..worktrees + 2 {
            let path = if w == 0 {
                main.clone()
            } else {
                dir.join(format!("repository-{r}-feature-branch-{w}"))
            };
            std::fs::create_dir_all(&path).unwrap();
            let branch = if w == 0 {
                "main".to_string()
            } else {
                format!("feature/long-branch-name-{w}")
            };
            porcelain.push_str(&format!(
                "worktree {}\0HEAD 0123456789012345678901234567890123456789\0branch refs/heads/{branch}\0\0",
                path.display()
            ));
            if w < worktrees {
                let project = desk.open_in(workspace, &path);
                desk.set_repository(project, Some(common.clone()));
                seats.push(guild_session::Seat { workspace, project });
            }
        }
        let mut list = crate::git::worktrees::parse(porcelain.as_bytes()).unwrap();
        for (i, w) in list.iter_mut().enumerate() {
            w.committed = Some(1_700_000_000 + i as i64);
            w.lines = Some((120 + i as u32, 30));
            w.main_branch = i == 0;
            w.push = w
                .branch
                .clone()
                .map(|branch| crate::git::worktrees::Pushed {
                    remote: "origin".into(),
                    url: Some(format!(
                        "git@gitlab.example.invalid:team/repository-{r}.git"
                    )),
                    branch,
                });
        }
        lists.push((common, list));
    }
    desk.select_workspace(workspace);
    let mut sessions = Vec::new();
    for seat in &seats {
        for t in 0..tabs {
            let id = desk.add_session(SessionRecord::new(
                format!("Agent session {t}"),
                *seat,
                dir.into(),
                Runs::Claude(SessionMode::Terminal),
            ));
            sessions.push((*seat, id));
        }
    }
    cx.new(|cx| {
        let mut root = Root::from_state_with_policy(
            guild_session::Settings {
                autosave: false,
                ..Default::default()
            },
            desk,
            Policy::Diagnostic {
                state_dir: crate::paths::state_dir(),
            },
            cx,
        );
        root.isolated_render = true;
        for (common, list) in lists {
            root.worktrees.insert(common, list);
        }
        root.active_workspace = workspace;
        root.restored_workspaces.insert(workspace);
        for (seat, id) in &sessions {
            root.adopt(
                *seat,
                seat.project,
                crate::Body::Broken {
                    session: *id,
                    reason: "Fixture without a provider".into(),
                },
                cx,
            );
        }
        root.active_project = seats.first().map(|s| s.project);
        root
    })
}

fn mr(iid: u64) -> MergeRequest {
    MergeRequest {
        iid,
        title: format!("Change {iid}"),
        web_url: format!("https://gitlab.example.invalid/team/app/-/merge_requests/{iid}"),
        state: State::Open,
        target_branch: "main".into(),
        draft: false,
        conflict: false,
        mergeable: false,
        needs_rebase: false,
        checking: false,
        block: None,
        approvals: None,
        unresolved_threads: Some(2),
        pipeline: None,
        updated_at: None,
        finished_at: None,
    }
}

fn answer(ask: &Ask, iid: u64) -> Answer {
    let project = |c: &Candidate| {
        c.url
            .trim_start_matches("git@gitlab.example.invalid:")
            .trim_end_matches(".git")
            .to_owned()
    };
    Answer {
        repo: ask.repo.clone(),
        candidates: ask.candidates.clone(),
        generation: ask.generation,
        resolutions: ask
            .candidates
            .iter()
            .map(|c| {
                (
                    c.clone(),
                    Resolution::Asked {
                        host: "gitlab.example.invalid".into(),
                        project: project(c),
                    },
                )
            })
            .collect(),
        batches: vec![Batch {
            host: "gitlab.example.invalid".into(),
            project: project(&ask.candidates[0]),
            result: Ok(ask
                .candidates
                .iter()
                .map(|c| (c.branch.clone(), Ok(Some(mr(iid)))))
                .collect()),
        }],
    }
}

/// Every repository's first answer, landed: the badges are drawn.
pub fn land_all(root: &Entity<Root>, now: Instant, cx: &mut App) {
    root.update(cx, |root, cx| {
        root.settings.gitlab_merge_requests = true;
        root.forge_model_mut().set_enabled(true, now);
        root.forge_reconcile();
        let model = root.forge_model_mut();
        for ask in model.take_due(now) {
            model.land(answer(&ask, 1), now);
        }
        cx.notify();
    });
}

/// Repository `r`'s next answer, with MR `iid` on each branch, landed at
/// `now` and notified as `notify` says. Whether it notified.
pub fn land_one(
    root: &Entity<Root>,
    dir: &Path,
    r: usize,
    iid: u64,
    now: Instant,
    notify: Notify,
    cx: &mut App,
) -> bool {
    root.update(cx, |root, cx| {
        let repo = repository(dir, r).join(".git");
        let model = root.forge_model_mut();
        model.trigger(Some(&repo), Trigger::Tick, now);
        let Some(ask) = model.take_due(now).pop() else {
            return false;
        };
        let changed = model.land(answer(&ask, iid), now);
        let notified = changed || notify == Notify::Always;
        if notified {
            cx.notify();
        }
        notified
    })
}

/// A notify of the root alone: the frame every notify costs.
pub fn notify(root: &Entity<Root>, cx: &mut App) {
    root.update(cx, |_, cx| cx.notify());
}
