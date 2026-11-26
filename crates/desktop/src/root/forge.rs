//! The window's half of a worktree's merge request — GLD-063.
//!
//! [`crate::forge::Forge`] decides when a repository is asked; this owns
//! the asking: one thread per refresh, off the window's thread, its glab
//! children admitted globally. The proxy is applied by `crate::network`
//! under `Scope::Guild`, and nowhere else.
//!
//! **Every refresh has an owner until its thread has ended** (code review
//! r1, bugs+impl-1 and bugs+impl-2). The setting going off or a repository
//! leaving the column *stops* a refresh: nothing more starts, a glab
//! already running finishes — it may be saving a refreshed login — and its
//! answer is dropped. The window quitting *aborts* every refresh and waits
//! for each thread, whose glab children are ended, `SIGTERM` first, and
//! reaped before the quit completes.
use crate::{
    Root,
    forge::{Answer, Ask, Batch, Candidate, Resolution, Shown, Trigger, Why, resolve},
};
use gpui::Context;
use guild_session::merge_requests::{
    glab::{Cancel, Error, Glab, Limits},
    hosts::Hosts,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, TryRecvError},
    },
    time::Instant,
};

/// The merge requests' model and the refreshes out.
pub(crate) struct ForgeState {
    model: crate::forge::Forge,
    jobs: HashMap<PathBuf, Job>,
    /// A test's glab and hosts instead of the login PATH's and glab's own
    /// config: a test never reaches the person's glab.
    #[cfg(test)]
    stand_in: Option<(PathBuf, Hosts)>,
}

struct Job {
    cancel: Arc<Cancel>,
    answer: mpsc::Receiver<Answer>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Default for ForgeState {
    fn default() -> Self {
        Self {
            // Off until the settings are read on the first poll: nothing
            // is asked before the window knows the person's choice.
            model: crate::forge::Forge::new(false),
            jobs: HashMap::new(),
            #[cfg(test)]
            stand_in: None,
        }
    }
}

impl ForgeState {
    /// Stop every refresh: nothing more starts, running children finish.
    /// The jobs stay owned until their threads end.
    fn stop_all(&self) {
        for job in self.jobs.values() {
            job.cancel.stop();
        }
    }

    /// Abort every refresh and hand over its thread, for the quit to wait on.
    pub(crate) fn shutdown(&mut self) -> Vec<std::thread::JoinHandle<()>> {
        self.jobs
            .drain()
            .filter_map(|(_, mut job)| {
                job.cancel.abort();
                job.thread.take()
            })
            .collect()
    }
}

impl Drop for ForgeState {
    fn drop(&mut self) {
        // A fallback: the quit has already taken the threads.
        for job in self.jobs.values() {
            job.cancel.abort();
        }
    }
}

/// The verb of a write the Console recorded: its first word that is not a
/// `-c key=value` pair.
fn verb(argv: &[String]) -> Option<&str> {
    let mut words = argv.iter();
    while let Some(word) = words.next() {
        if word == "-c" {
            words.next();
            continue;
        }
        if !word.starts_with('-') {
            return Some(word);
        }
    }
    None
}

/// Whether a write asks its repository's MRs again: a push or a branch
/// switch made from Guild.
pub(crate) fn asks_again(line: &crate::console::Line) -> bool {
    !line.failed && matches!(verb(&line.argv), Some("push" | "switch"))
}

impl Root {
    /// On the watch loop's clock: follow the setting, take the lists in,
    /// land what came back, and start what is due.
    pub(crate) fn poll_forge(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let enabled = self.settings.gitlab_merge_requests;
        if self.forge.model.set_enabled(enabled, now) {
            if !enabled {
                self.forge.stop_all();
            }
            cx.notify();
        }
        // The rows as they stand now, before any reply is taken: a reply
        // whose candidates the latest list replaced publishes nothing (code
        // review r2, bugs+impl-6).
        if enabled {
            self.forge_take_in_the_column(now);
        }
        // Whether a landed answer changed what the rows or a repository's
        // state draw — not merely that one landed: each notify of the root
        // rebuilds the whole column.
        let mut changed = false;
        // Land, on or off: a thread that ended releases its job.
        let mut landed = Vec::new();
        for (repo, job) in &self.forge.jobs {
            match job.answer.try_recv() {
                Ok(answer) => landed.push((repo.clone(), Some(answer))),
                Err(TryRecvError::Disconnected) => landed.push((repo.clone(), None)),
                Err(TryRecvError::Empty) => {}
            }
        }
        for (repo, answer) in landed {
            self.forge.jobs.remove(&repo);
            match answer {
                Some(answer) => changed |= self.forge.model.land(answer, now),
                None => self.forge.model.abandon(&repo),
            }
        }
        if !enabled {
            return;
        }
        if self.window_focused {
            self.forge.model.trigger(None, Trigger::Tick, now);
        }
        for ask in self.forge.model.take_due(now) {
            self.start_forge_refresh(ask);
        }
        if changed {
            cx.notify();
        }
    }

    /// Take the column in now, when the setting is on: the desk changed —
    /// a workspace switched, a project opened or closed.
    pub(crate) fn forge_reconcile(&mut self) {
        if self.settings.gitlab_merge_requests && self.forge.model.enabled() {
            self.forge_take_in_the_column(Instant::now());
        }
    }

    /// The candidates of the repositories the active workspace's column
    /// shows, and only those (code review r2, docs+tests-4): a repository
    /// open only in another workspace is hidden in the model — its refresh
    /// stopped, its last known MRs kept — until that workspace is the one
    /// shown.
    fn forge_take_in_the_column(&mut self, now: Instant) {
        let workspace = self.desk.active_workspace();
        let mut shown: Vec<PathBuf> = Vec::new();
        for group in self.column_repositories_for(workspace) {
            let Some(repo) = group.repository.clone() else {
                continue;
            };
            let rows: Vec<PathBuf> = group
                .rows
                .iter()
                .filter_map(|row| self.column_row_path(row))
                .collect();
            let candidates = self
                .worktrees
                .get(&repo)
                .into_iter()
                .flatten()
                // The main branch is never asked: no glab call, nothing on
                // its row.
                .filter(|tree| tree.shown() && !tree.main_branch && rows.contains(&tree.path))
                .filter_map(|tree| {
                    let push = tree.push.as_ref()?;
                    Some(Candidate {
                        worktree: tree.path.clone(),
                        url: push.url.clone()?,
                        branch: push.branch.clone(),
                    })
                })
                .collect();
            self.forge.model.inputs(&repo, candidates, now);
            shown.push(repo);
        }
        // A repository that left the column is hidden and stops its
        // refresh; the job is kept until its thread ends, and none starts
        // beside it.
        for gone in self
            .forge
            .model
            .retain(|repo| shown.iter().any(|kept| kept == repo))
        {
            if let Some(job) = self.forge.jobs.get(&gone) {
                job.cancel.stop();
            }
        }
    }

    /// The window came back: ask again what has not been asked lately.
    pub(crate) fn forge_focused(&mut self) {
        self.forge
            .model
            .trigger(None, Trigger::Focus, Instant::now());
    }

    /// A push or a branch switch made from Guild in `project` was carried
    /// out: its repository is asked again once the list after it lands.
    pub(crate) fn forge_wrote(&mut self, project: guild_session::ProjectId) {
        let repo = self
            .desk
            .project(project)
            .and_then(|project| project.repository.clone());
        if let Some(repo) = repo {
            self.forge.model.wrote(&repo, Instant::now());
        }
    }

    /// A repository's worktree list has landed — the snapshot a write
    /// made from Guild waits for.
    pub(crate) fn forge_snapshot_landed(&mut self, repo: &Path) {
        self.forge.model.snapshot_landed(repo);
    }

    /// Abort every refresh, for the quit: the threads to wait on.
    pub(crate) fn forge_shutdown(&mut self) -> Vec<std::thread::JoinHandle<()>> {
        self.forge.shutdown()
    }

    /// The model, for a test or a native probe that lands an answer by hand.
    #[cfg(any(test, feature = "native-preview"))]
    pub(crate) fn forge_model_mut(&mut self) -> &mut crate::forge::Forge {
        &mut self.forge.model
    }

    /// What a worktree's row draws.
    pub(crate) fn forge_shown(&self, worktree: &Path) -> Shown<'_> {
        self.forge.model.shown(worktree)
    }

    /// A repository's state, by its common git directory, for its header:
    /// queued, checking, when it was last answered, and what could not be
    /// asked at all. `None` while the setting is off or the column does not
    /// show it.
    #[allow(dead_code)] // The repository header's indicator reads it.
    pub(crate) fn forge_status(&self, repo: &Path) -> Option<crate::forge::RepoStatus<'_>> {
        self.forge.model.status(repo)
    }

    /// The URL of the MR a worktree's row shows now, if it shows one.
    pub(crate) fn forge_url(&self, worktree: &Path) -> Option<String> {
        match self.forge_shown(worktree) {
            Shown::Mr { mr, .. } => Some(mr.web_url.clone()),
            _ => None,
        }
    }

    /// `cmd-alt-g`: the active worktree's MR in the browser; nothing
    /// without one.
    pub(crate) fn view_active_merge_request(&mut self, cx: &mut Context<Self>) {
        let url = self
            .active_project
            .and_then(|project| self.desk.project(project))
            .and_then(|project| self.forge_url(&project.path));
        if let Some(url) = url {
            cx.open_url(&url);
        }
    }

    fn start_forge_refresh(&mut self, ask: Ask) {
        let cancel = Arc::new(Cancel::default());
        let (send, answer) = mpsc::channel();
        let proxy = crate::network::policy(guild_session::network::Scope::Guild);
        let thread_cancel = cancel.clone();
        let repo = ask.repo.clone();
        #[cfg(test)]
        let stand_in = self.forge.stand_in.clone();
        #[cfg(not(test))]
        let stand_in: Option<(PathBuf, Hosts)> = None;
        let thread = std::thread::spawn(move || {
            let (located, hosts) = match stand_in {
                Some((program, hosts)) => (Ok(program), hosts),
                None => (
                    Glab::locate(&guild_session::login_path::path_or_inherited()),
                    Hosts::of_this_process(),
                ),
            };
            let budget = Limits::default().refresh;
            let _ = send.send(refresh(ask, located, &hosts, proxy, thread_cancel, budget));
        });
        self.forge.jobs.insert(
            repo,
            Job {
                cancel,
                answer,
                thread: Some(thread),
            },
        );
    }
}

/// One logical refresh of one repository, under **one deadline** for all
/// its batches (code review r1, bugs+impl-7): a batch that has no time
/// left is timed out as a whole without a call — the repository's problem
/// — and its rows keep their last known state. A failure of glab's or of
/// the network policy fails a batch as a whole too; a branch's own failure
/// stays the branch's.
pub(crate) fn refresh(
    ask: Ask,
    located: Result<PathBuf, Error>,
    hosts: &Hosts,
    proxy: Result<Option<guild_session::network::Proxy>, &'static str>,
    cancel: Arc<Cancel>,
    budget: std::time::Duration,
) -> Answer {
    let deadline = Instant::now() + budget;
    let (mut resolutions, asked) = resolve(&ask.candidates, located.is_ok(), hosts);
    let mut batches = Vec::new();
    // A host GitLab answered 401 for is signed out for the rest of the
    // refresh: one probe, not one per project (code review r2, bugs+impl-2).
    let mut signed_out: Vec<String> = Vec::new();
    if let Ok(program) = located {
        for (host, project, branches) in asked {
            if cancel.is_raised() {
                break;
            }
            if signed_out.contains(&host) {
                continue;
            }
            let result = match &proxy {
                Err(why) => Err(Error::Failed((*why).to_owned())),
                // glab's own proxy for the host wins over the environment's,
                // so Guild's Custom proxy would not apply: not asked (code
                // review r1, arch+quality-2). In System mode Guild leaves
                // the configuration as it is, glab's included.
                Ok(Some(_)) if glab_proxy(hosts, &host) => Err(Error::Failed(format!(
                    "glab has a proxy of its own for {host}, which would override Guild's network settings"
                ))),
                Ok(proxy) => {
                    let proxy = proxy.clone();
                    Glab::at(program.clone())
                        .with_cancel(cancel.clone())
                        .with_prepare(Arc::new(move |command| {
                            crate::network::apply_policy(command, proxy.as_ref());
                        }))
                        .read_until(&host, &project, &branches, deadline)
                }
            };
            match result {
                Ok(found) => batches.push(Batch {
                    host,
                    project,
                    result: Ok(found),
                }),
                // glab's login turned out not to hold: the rows say so,
                // as for a host glab lists with no credential.
                Err(Error::SignedOut { .. }) => {
                    for (_, resolution) in &mut resolutions {
                        if matches!(resolution, Resolution::Asked { host: h, .. } if *h == host) {
                            *resolution = Resolution::Why(Why::NotLoggedIn(host.clone()));
                        }
                    }
                    signed_out.push(host);
                }
                Err(error) => batches.push(Batch {
                    host,
                    project,
                    result: Err(error),
                }),
            }
        }
    }
    Answer {
        repo: ask.repo,
        candidates: ask.candidates,
        generation: ask.generation,
        resolutions,
        batches,
    }
}

/// Whether glab keeps a proxy of its own for a glab host.
fn glab_proxy(hosts: &Hosts, host: &str) -> bool {
    hosts.proxy(host).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(argv: &[&str], failed: bool) -> crate::console::Line {
        crate::console::Line {
            at: 0,
            root: String::new(),
            argv: argv.iter().map(|w| (*w).to_owned()).collect(),
            said: String::new(),
            failed,
        }
    }

    /// Only a push or a switch that git carried out asks again; a path
    /// named `push` in an add is not a push.
    #[test]
    fn a_push_or_a_switch_asks_again() {
        assert!(asks_again(&line(&["push", "origin", "feat"], false)));
        assert!(asks_again(&line(&["switch", "-c", "new"], false)));
        assert!(asks_again(&line(&["-c", "a=b", "push"], false)));
        assert!(!asks_again(&line(&["push"], true)));
        assert!(!asks_again(&line(&["add", "--", "push"], false)));
        assert!(!asks_again(&line(&["commit", "-F", "-"], false)));
    }

    // --- code review r2

    /// A stand-in glab: logs each call, answers `[]`, or what `mode` says.
    fn stand_in(mode: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("glab");
        let body = format!(
            "#!/bin/sh\ndir=\"$(dirname \"$0\")\"\nprintf '%s\\n' \"$*\" >> \"$dir/calls.log\"\ntrap 'exit 143' TERM\n{mode}\necho '[]'\n"
        );
        std::fs::write(&script, body).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, script)
    }

    fn calls(dir: &tempfile::TempDir) -> Vec<String> {
        std::fs::read_to_string(dir.path().join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Two projects on one signed-in host, in one repository.
    fn two_projects() -> (Ask, Hosts) {
        let candidate = |worktree: &str, project: &str| Candidate {
            worktree: PathBuf::from(worktree),
            url: format!("git@gitlab.example.com:team/{project}.git"),
            branch: "feat".into(),
        };
        (
            Ask {
                repo: PathBuf::from("/work/app/.git"),
                candidates: vec![candidate("/work/one", "one"), candidate("/work/two", "two")],
                generation: 0,
            },
            Hosts::parse("hosts:\n    gitlab.example.com:\n        token: present\n"),
        )
    }

    /// docs+tests-8: one budget for the repository's whole refresh: a first
    /// batch that spends it leaves the second none, and the second starts no
    /// child. The budget leaves a loaded machine room to start the first
    /// child and log its call: 500 ms did not, and the call went unlogged.
    #[test]
    fn one_budget_bounds_every_batch_of_a_refresh() {
        let (dir, glab) = stand_in("sleep 10 & wait $!");
        let (ask, hosts) = two_projects();
        let started = Instant::now();
        let answer = refresh(
            ask,
            Ok(glab),
            &hosts,
            Ok(None),
            Arc::new(Cancel::default()),
            std::time::Duration::from_secs(3),
        );
        assert_eq!(calls(&dir).len(), 1, "{:?}", calls(&dir));
        assert_eq!(answer.batches.len(), 2);
        // The first batch's branch timed out in its call — its own; the
        // second had no time left before it started — the repository's.
        assert_eq!(
            answer.batches[0].result,
            Ok(vec![("feat".to_owned(), Err(Error::TimedOut))])
        );
        assert_eq!(answer.batches[1].result, Err(Error::TimedOut));
        assert!(started.elapsed() < std::time::Duration::from_secs(8));
    }

    /// bugs+impl-2: a 401 says the host is signed out for the rest of the
    /// refresh: its other projects are not asked, and every candidate on it
    /// resolves to the reason — the repository's state, not its rows'.
    #[test]
    fn a_401_signs_the_host_out_for_the_whole_refresh() {
        let (dir, glab) = stand_in("echo 'glab: 401 Unauthorized (HTTP 401)' >&2; exit 1");
        let (ask, hosts) = two_projects();
        let answer = refresh(
            ask,
            Ok(glab),
            &hosts,
            Ok(None),
            Arc::new(Cancel::default()),
            std::time::Duration::from_secs(20),
        );
        assert_eq!(calls(&dir).len(), 1, "one probe: {:?}", calls(&dir));
        assert!(answer.batches.is_empty(), "{:?}", answer.batches);
        for (_, resolution) in &answer.resolutions {
            assert_eq!(
                resolution,
                &Resolution::Why(Why::NotLoggedIn("gitlab.example.com".into()))
            );
        }
    }
    use gpui::AppContext as _;

    fn worktree(path: &Path, branch: &str, project: &str) -> crate::git::worktrees::Worktree {
        crate::git::worktrees::Worktree {
            path: path.to_path_buf(),
            branch: Some(branch.into()),
            head: Some("abc".into()),
            push: Some(crate::git::worktrees::Pushed {
                remote: "origin".into(),
                url: Some(format!("git@gitlab.example.com:team/{project}.git")),
                branch: branch.into(),
            }),
            ..Default::default()
        }
    }

    fn signed_in() -> Hosts {
        Hosts::parse("hosts:\n    gitlab.example.com:\n        token: present\n")
    }

    /// Poll until every refresh out has landed, or five seconds.
    fn settle(root: &gpui::Entity<Root>, cx: &mut gpui::TestAppContext) {
        let until = Instant::now() + std::time::Duration::from_secs(5);
        loop {
            root.update(cx, |root, cx| root.poll_forge(cx));
            cx.run_until_parked();
            let idle = root.read_with(cx, |root, _| root.forge.jobs.is_empty());
            if idle || Instant::now() >= until {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// docs+tests-4: only the repositories the active workspace's column
    /// shows are asked; a repository open only in another workspace is not,
    /// until that workspace is the one shown.
    #[gpui::test]
    fn only_the_shown_workspace_is_asked(cx: &mut gpui::TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::only_the_shown_workspace_is_asked"
        )) {
            return;
        }
        // A's list takes a while and names an open MR, whose detail the
        // read would ask next.
        let (dir, glab) = stand_in(
            r#"case "$*" in *"team%2Fa/merge_requests?source_branch"*) sleep 0.6; echo '[{"iid":1,"title":"A","web_url":"https://gitlab.example.com/team/a/-/merge_requests/1","sha":"h1","source_branch":"feat","target_branch":"main","source_project_id":7,"project_id":7,"state":"opened"}]'; exit 0;; esac"#,
        );
        let base = tempfile::tempdir().unwrap();
        let (a, b) = (base.path().join("a"), base.path().join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let mut two = None;
        let root = cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let one = desk.active_workspace();
            let project = desk.open_in(one, &a);
            desk.set_repository(project, Some(a.join(".git")));
            let other = desk.add_workspace("Two", None).unwrap();
            let project = desk.open_in(other, &b);
            desk.set_repository(project, Some(b.join(".git")));
            desk.select_workspace(one);
            two = Some(other);
            cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.settings.gitlab_merge_requests = true;
                root.forge.stand_in = Some((glab.clone(), signed_in()));
                root.worktrees
                    .insert(a.join(".git"), vec![worktree(&a, "feat", "a")]);
                root.worktrees
                    .insert(b.join(".git"), vec![worktree(&b, "feat", "b")]);
                root
            })
        });
        root.update(cx, |root, cx| root.poll_forge(cx));
        let until = Instant::now() + std::time::Duration::from_secs(5);
        while calls(&dir).is_empty() && Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let asked = calls(&dir);
        assert!(asked.iter().any(|c| c.contains("team%2Fa")), "{asked:?}");
        assert!(!asked.iter().any(|c| c.contains("team%2Fb")), "{asked:?}");

        // Code review r3, lenses-6: the switch itself revokes A's refresh,
        // while its first call runs and before any poll: that call
        // finishes, and nothing more is asked for A.
        root.update(cx, |root, cx| root.select_workspace(two.unwrap(), cx));
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let asked = calls(&dir);
        assert!(
            !asked
                .iter()
                .any(|c| c.contains("team%2Fa/merge_requests/1")),
            "a hidden repository asked again: {asked:?}"
        );
        settle(&root, cx);
        let asked = calls(&dir);
        assert!(asked.iter().any(|c| c.contains("team%2Fb")), "{asked:?}");
    }

    /// A reply that brings exactly what the rows show already does not
    /// notify the root — every notify of the root rebuilds the whole
    /// column (docs/traps.md) — and one that changes a row does.
    #[gpui::test]
    fn an_unchanged_reply_does_not_notify_the_root(cx: &mut gpui::TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::an_unchanged_reply_does_not_notify_the_root"
        )) {
            return;
        }
        let (_dir, glab) = stand_in("");
        let base = tempfile::tempdir().unwrap();
        let a = base.path().join("a");
        std::fs::create_dir_all(&a).unwrap();
        let common = a.join(".git");
        let root = cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let one = desk.active_workspace();
            let project = desk.open_in(one, &a);
            desk.set_repository(project, Some(common.clone()));
            cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.settings.gitlab_merge_requests = true;
                root.forge.stand_in = Some((glab.clone(), signed_in()));
                root.worktrees
                    .insert(common.clone(), vec![worktree(&a, "feat", "app")]);
                root
            })
        });
        let notified = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let seen = notified.clone();
        cx.update(|cx| {
            cx.observe(&root, move |_, _| seen.set(seen.get() + 1))
                .detach()
        });
        let mr = |iid: u64| guild_session::merge_requests::status::MergeRequest {
            iid,
            title: "A".into(),
            web_url: format!("https://gitlab.example.com/team/app/-/merge_requests/{iid}"),
            state: guild_session::merge_requests::status::State::Open,
            target_branch: "main".into(),
            draft: false,
            conflict: false,
            mergeable: true,
            needs_rebase: false,
            checking: false,
            block: None,
            approvals: None,
            unresolved_threads: Some(0),
            pipeline: None,
            updated_at: None,
            finished_at: None,
        };
        // A reply waiting in its job's channel, landed by the next poll.
        let reply = |root: &mut Root, iid: u64, at: Instant| {
            root.forge.jobs.clear();
            let model = &mut root.forge.model;
            model.abandon(&common);
            model.trigger(None, Trigger::Tick, at);
            let ask = model.take_due(at).remove(0);
            let (send, answer) = mpsc::channel();
            send.send(Answer {
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
                                host: "gitlab.example.com".into(),
                                project: "team/app".into(),
                            },
                        )
                    })
                    .collect(),
                batches: vec![Batch {
                    host: "gitlab.example.com".into(),
                    project: "team/app".into(),
                    result: Ok(vec![("feat".into(), Ok(Some(mr(iid))))]),
                }],
            })
            .unwrap();
            root.forge.jobs.insert(
                common.clone(),
                Job {
                    cancel: Arc::new(Cancel::default()),
                    answer,
                    thread: None,
                },
            );
        };
        // The setting read, the column taken in, a refresh started.
        root.update(cx, |root, cx| root.poll_forge(cx));
        cx.run_until_parked();
        let start = Instant::now();
        let mut landed = |iid: u64, at: Instant| {
            notified.set(0);
            root.update(cx, |root, cx| {
                reply(root, iid, at);
                root.poll_forge(cx);
            });
            cx.run_until_parked();
            notified.get()
        };
        assert_eq!(landed(1, start), 1, "the first MR is drawn");
        assert_eq!(
            landed(1, start + std::time::Duration::from_secs(200)),
            0,
            "the same MR again asks for no frame"
        );
        assert_eq!(
            landed(2, start + std::time::Duration::from_secs(400)),
            1,
            "another MR is drawn"
        );
    }

    /// The worktree on the repository's main branch is not asked: glab is
    /// called for the feature branch only, and the main branch's row has
    /// nothing to show.
    #[gpui::test]
    fn the_main_branch_is_not_asked(cx: &mut gpui::TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::the_main_branch_is_not_asked"
        )) {
            return;
        }
        let (dir, glab) = stand_in("");
        let base = tempfile::tempdir().unwrap();
        let (a, b) = (base.path().join("a"), base.path().join("b"));
        std::fs::create_dir_all(&a).unwrap();
        let common = a.join(".git");
        let root = cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let one = desk.active_workspace();
            let project = desk.open_in(one, &a);
            desk.set_repository(project, Some(common.clone()));
            cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.settings.gitlab_merge_requests = true;
                root.forge.stand_in = Some((glab.clone(), signed_in()));
                let main = crate::git::worktrees::Worktree {
                    main_branch: true,
                    ..worktree(&a, "main", "app")
                };
                root.worktrees
                    .insert(common.clone(), vec![main, worktree(&b, "feat", "app")]);
                root
            })
        });
        settle(&root, cx);
        let asked = calls(&dir);
        assert!(
            asked.iter().any(|c| c.contains("source_branch=feat")),
            "{asked:?}"
        );
        assert!(
            !asked.iter().any(|c| c.contains("source_branch=main")),
            "{asked:?}"
        );
        root.read_with(cx, |root, _| {
            assert_eq!(root.forge_shown(&a), Shown::Nothing);
        });
    }

    /// bugs+impl-6: a reply and the worktree list that replaced its
    /// candidates land in the same turn: the list is taken in first, and
    /// the reply, no longer the rows', publishes nothing.
    #[gpui::test]
    fn the_list_is_taken_in_before_a_reply_of_the_same_turn(cx: &mut gpui::TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::the_list_is_taken_in_before_a_reply_of_the_same_turn"
        )) {
            return;
        }
        let (_dir, glab) = stand_in("");
        let base = tempfile::tempdir().unwrap();
        let (a, b) = (base.path().join("a"), base.path().join("b"));
        std::fs::create_dir_all(&a).unwrap();
        let common = a.join(".git");
        let root = cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let one = desk.active_workspace();
            let project = desk.open_in(one, &a);
            desk.set_repository(project, Some(common.clone()));
            cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.settings.gitlab_merge_requests = true;
                root.forge.stand_in = Some((glab.clone(), signed_in()));
                root.worktrees.insert(
                    common.clone(),
                    vec![worktree(&a, "a", "app"), worktree(&b, "b", "app")],
                );
                root
            })
        });
        let now = Instant::now();
        root.update(cx, |root, cx| {
            // A's MR read once, and the next refresh of A and B out.
            root.poll_forge(cx);
            root.forge.jobs.clear();
            let model = &mut root.forge.model;
            model.abandon(&common);
            let ask = model.take_due(now).remove(0);
            let mr = guild_session::merge_requests::status::MergeRequest {
                iid: 1,
                title: "A".into(),
                web_url: "https://gitlab.example.com/team/app/-/merge_requests/1".into(),
                state: guild_session::merge_requests::status::State::Open,
                target_branch: "main".into(),
                draft: false,
                conflict: false,
                mergeable: true,
                needs_rebase: false,
                checking: false,
                block: None,
                approvals: None,
                unresolved_threads: Some(0),
                pipeline: None,
                updated_at: None,
                finished_at: None,
            };
            let asked = |candidate: &Candidate| {
                (
                    candidate.clone(),
                    Resolution::Asked {
                        host: "gitlab.example.com".into(),
                        project: "team/app".into(),
                    },
                )
            };
            model.land(
                Answer {
                    repo: ask.repo.clone(),
                    candidates: ask.candidates.clone(),
                    generation: ask.generation,
                    resolutions: ask.candidates.iter().map(asked).collect(),
                    batches: vec![Batch {
                        host: "gitlab.example.com".into(),
                        project: "team/app".into(),
                        result: Ok(vec![("a".into(), Ok(Some(mr))), ("b".into(), Ok(None))]),
                    }],
                },
                now,
            );
            let later = now + std::time::Duration::from_secs(200);
            model.trigger(None, Trigger::Tick, later);
            let ask = model.take_due(later).remove(0);
            // Its reply, a failure, is waiting in the channel…
            let (send, answer) = mpsc::channel();
            send.send(Answer {
                repo: ask.repo.clone(),
                candidates: ask.candidates.clone(),
                generation: ask.generation,
                resolutions: ask.candidates.iter().map(asked).collect(),
                batches: vec![Batch {
                    host: "gitlab.example.com".into(),
                    project: "team/app".into(),
                    result: Err(Error::TimedOut),
                }],
            })
            .unwrap();
            root.forge.jobs.insert(
                common.clone(),
                Job {
                    cancel: Arc::new(Cancel::default()),
                    answer,
                    thread: None,
                },
            );
            // …when the list moves B to another project.
            root.worktrees.insert(
                common.clone(),
                vec![worktree(&a, "a", "app"), worktree(&b, "b", "other")],
            );
            root.poll_forge(cx);
            match root.forge_shown(&a) {
                Shown::Mr { failure, .. } => {
                    assert_eq!(failure, None, "the old batch's failure is not A's")
                }
                other => panic!("{other:?}"),
            }
        });
    }
}
