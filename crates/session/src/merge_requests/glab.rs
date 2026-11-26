//! The GitLab reader: `glab api`, reads only — GLD-063.
//!
//! Cut down from the removed MR panel's reader (commit 69c459cf): every
//! call is `glab api --hostname <host> <endpoint>` with `GLAB_NO_PROMPT`,
//! in a process group of its own, under a per-call and a whole-read
//! deadline, its output capped while it runs. **Each branch is whole or
//! nothing**: a failed page, a head that moved between the list and the
//! detail, or a bound passed answers that branch's error, the caller keeps
//! what it had for it, and the other branches stand on their own. Only what
//! is not a branch's — a stop, glab gone, a 401, the deadline spent before
//! the first branch — fails the read as a whole.
//!
//! What did not come back: the five writes, the account read, and project
//! discovery through glab's `projects/:id` placeholder — the host and the
//! project path come from the worktree's remote and are passed explicitly.
//!
//! Credentials stay glab's. The child's environment is scrubbed of every
//! endpoint and credential override ([`SCRUBBED`]), so the answer depends
//! on glab's own login for the host and on nothing Guild was started with.
use super::status::{self, Approvals, MergeRequest, State};
use crate::process::ProcessGroup;
use serde::Deserialize;
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// The largest answer read.
const CAP: u64 = 8 * 1024 * 1024;
/// The most of glab's diagnostics read.
const DIAGNOSTICS_CAP: u64 = 64 * 1024;
/// Pages of one list read at most: 2 000 entries.
const PAGES: u32 = 20;
/// The longest error line shown.
const LINE: usize = 200;
/// glab children at once, across every repository (spec review r2,
/// finding 4): glab children take none of the four git read permits.
pub const ADMITTED: usize = 2;

/// Inherited variables removed from every glab child (spec review r2,
/// finding 1). `--hostname` picks glab's login host, but its API client
/// resolves the endpoint separately, so an inherited override could send
/// the host's authorization header elsewhere:
///
/// - endpoint and TLS overrides glab 1.118 reads for a host;
/// - credentials from the environment, bound to no host — the source is
///   glab's own login (decided 2026-11-25);
/// - debug switches, which can print request headers into the
///   diagnostics read back.
///
/// `GLAB_CONFIG_DIR` is kept: it says where glab's own login lives.
pub const SCRUBBED: &[&str] = &[
    // glab's own proxy wins over the environment's, where Guild's
    // `crate::network` choice is (code review r1, arch+quality-2).
    "GLAB_PROXY",
    "PROXY",
    // The base path, and where CI would take it from.
    "GITLAB_SUBFOLDER",
    "CI_SERVER_URL",
    "CI_SERVER_SHELL_SSH_HOST",
    "CI_SERVER_SHELL_SSH_PORT",
    "GLAB_IS_OAUTH2",
    "GITLAB_HOST",
    "GITLAB_URI",
    "GITLAB_URL",
    "GITLAB_API_HOST",
    "GITLAB_API_URL",
    "GL_HOST",
    "GLAB_API_PROTOCOL",
    "API_PROTOCOL",
    "GLAB_CA_CERT",
    "CA_CERT",
    "GLAB_CLIENT_CERT",
    "CLIENT_CERT",
    "GLAB_CLIENT_KEY",
    "CLIENT_KEY",
    "GLAB_SKIP_TLS_VERIFY",
    "SKIP_TLS_VERIFY",
    "GITLAB_TOKEN",
    "GITLAB_ACCESS_TOKEN",
    "GITLAB_PERSONAL_ACCESS_TOKEN",
    "OAUTH_TOKEN",
    "GITLAB_CLIENT_ID",
    "GLAB_ENABLE_CI_AUTOLOGIN",
    "CI_JOB_TOKEN",
    "DEBUG",
    "GLAB_DEBUG",
    "GLAB_DEBUG_HTTP",
];

/// Each branch's MR, or why that branch alone has no answer — what
/// [`Glab::read`] answers when the read as a whole held.
pub type Answers = Vec<(String, Result<Option<MergeRequest>, Error>)>;

/// Why glab gave no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// `glab` is not on the login PATH.
    NotInstalled,
    /// glab is not signed in to the host, or GitLab answered 401.
    SignedOut {
        /// The glab host.
        host: String,
    },
    /// A call or the whole read passed its deadline.
    TimedOut,
    /// An MR or a page changed while it was read; a later read may succeed.
    Changed,
    /// The read was stopped by its owner.
    Stopped,
    /// Anything else, as one bounded line free of credentials.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("glab is not installed"),
            Self::SignedOut { host } => write!(f, "glab is not logged in to {host}"),
            Self::TimedOut => f.write_str("glab did not answer in time"),
            Self::Changed => f.write_str("The merge requests changed while they were read"),
            Self::Stopped => f.write_str("Stopped"),
            Self::Failed(line) => f.write_str(line),
        }
    }
}

/// How a read's owner ends it. **Stop** (the setting off, a repository
/// closed) starts nothing more and lets a child already running finish —
/// glab may be saving a refreshed login — then drops its answer. **Abort**
/// (the window quitting) also ends the child in flight, `SIGTERM` first.
///
/// A child is started under the same lock a stop is raised under, so once
/// [`Cancel::stop`] has returned no child of this read can start (code
/// review r1, docs+tests-3).
#[derive(Debug, Default)]
pub struct Cancel {
    level: Mutex<Level>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Level {
    #[default]
    Run,
    Stop,
    Abort,
}

impl Cancel {
    /// Start nothing more; let a running child finish and drop its answer.
    pub fn stop(&self) {
        let mut level = self.level.lock().unwrap_or_else(|p| p.into_inner());
        if *level == Level::Run {
            *level = Level::Stop;
        }
    }

    /// End the child in flight too.
    pub fn abort(&self) {
        *self.level.lock().unwrap_or_else(|p| p.into_inner()) = Level::Abort;
    }

    /// Whether a stop or an abort was raised.
    pub fn is_raised(&self) -> bool {
        self.level() != Level::Run
    }

    fn level(&self) -> Level {
        *self.level.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Start `command` unless a stop was raised or `deadline` has passed,
    /// both looked at under the stop's lock, right before the child (code
    /// review r2, bugs+impl-12).
    ///
    /// The child is put in the live list before the lock is let go, so an
    /// abort — which takes this lock — returns only once a child started
    /// under it can be killed by [`kill_every_child`] (code review r3,
    /// lenses-1).
    fn spawn(
        &self,
        command: &mut Command,
        deadline: Instant,
    ) -> Result<(ProcessGroup, Live), Failure> {
        let level = self.level.lock().unwrap_or_else(|p| p.into_inner());
        if *level != Level::Run {
            return Err(Failure::Stopped);
        }
        if Instant::now() >= deadline {
            return Err(Failure::TimedOut);
        }
        match ProcessGroup::spawn(command) {
            Ok(process) => {
                #[cfg(test)]
                tests::at_spawn_boundary(process.id());
                let live = Live::of(&process);
                drop(level);
                Ok((process, live))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(Failure::NotInstalled)
            }
            Err(error) => Err(Failure::Other(format!("Could not start glab: {error}"))),
        }
    }
}

/// How long glab is waited for.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// One call.
    pub call: Duration,
    /// A whole read of one batch.
    pub refresh: Duration,
    /// How long a child asked to end with `SIGTERM` at its deadline or its
    /// cap has before it is killed: glab may be saving a refreshed OAuth
    /// login (code review r1, arch+quality-1).
    pub grace: Duration,
    /// The same, when the window is quitting: GPUI waits 200 ms for a
    /// quit's cleanup, and a child still alive after this is killed by
    /// [`kill_every_child`] (code review r2, bugs+impl-1).
    pub abort_grace: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            call: Duration::from_secs(10),
            refresh: Duration::from_secs(45),
            grace: Duration::from_secs(2),
            abort_grace: Duration::from_millis(100),
        }
    }
}

/// Permits for glab children, taken before a child's output files exist.
#[derive(Debug)]
pub struct Admission {
    limit: usize,
    taken: Mutex<usize>,
    freed: Condvar,
}

/// One admitted child; the permit goes back when it is dropped.
#[derive(Debug)]
pub struct Permit {
    admission: Arc<Admission>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut taken = self
            .admission
            .taken
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *taken = taken.saturating_sub(1);
        self.admission.freed.notify_one();
    }
}

impl Admission {
    /// At most `limit` children at once.
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            taken: Mutex::new(0),
            freed: Condvar::new(),
        }
    }

    /// The one every repository's reads share: [`ADMITTED`] at once.
    pub fn global() -> Arc<Self> {
        static GLOBAL: OnceLock<Arc<Admission>> = OnceLock::new();
        GLOBAL
            .get_or_init(|| Arc::new(Admission::new(ADMITTED)))
            .clone()
    }

    /// A permit now, or none.
    pub fn try_take(self: &Arc<Self>) -> Option<Permit> {
        let mut taken = self.taken.lock().unwrap_or_else(|p| p.into_inner());
        (*taken < self.limit).then(|| {
            *taken += 1;
            Permit {
                admission: self.clone(),
            }
        })
    }

    /// A permit before `deadline`, unless a stop is raised first.
    fn take(self: &Arc<Self>, deadline: Instant, cancel: &Cancel) -> Result<Permit, Failure> {
        let mut taken = self.taken.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if cancel.is_raised() {
                return Err(Failure::Stopped);
            }
            // The deadline before a permit: an expired read is granted
            // nothing, even with a slot free (code review r2, bugs+impl-12).
            let now = Instant::now();
            if now >= deadline {
                return Err(Failure::TimedOut);
            }
            if *taken < self.limit {
                *taken += 1;
                return Ok(Permit {
                    admission: self.clone(),
                });
            }
            // Woken by a returned permit, or every 50 ms to look at `stop`.
            let wait = (deadline - now).min(Duration::from_millis(50));
            taken = self
                .freed
                .wait_timeout(taken, wait)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }
}

/// How a caller prepares each child before it starts — the desktop's
/// proxy policy, applied by `crate::network` there and nowhere here.
pub type Prepare = Arc<dyn Fn(&mut Command) + Send + Sync>;

/// The reader. Cheap to clone; holds no connection.
#[derive(Clone)]
pub struct Glab {
    program: PathBuf,
    environment: Option<Vec<(OsString, OsString)>>,
    limits: Limits,
    cancel: Arc<Cancel>,
    admission: Arc<Admission>,
    prepare: Option<Prepare>,
}

impl Glab {
    /// glab on the login PATH: the first absolute directory that holds an
    /// executable `glab`, canonicalized — never a name for the child's own
    /// PATH to resolve (traps: *The `claude` on the `PATH` may be somebody
    /// else's*).
    pub fn locate(path: &str) -> Result<PathBuf, Error> {
        for directory in std::env::split_paths(OsStr::new(path)) {
            if !directory.is_absolute() {
                continue;
            }
            let candidate = directory.join("glab");
            if candidate
                .metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            {
                return candidate.canonicalize().map_err(|_| Error::NotInstalled);
            }
        }
        Err(Error::NotInstalled)
    }

    /// The glab at this absolute path, sharing the global admission.
    pub fn at(program: PathBuf) -> Self {
        Self {
            program,
            environment: None,
            limits: Limits::default(),
            cancel: Arc::new(Cancel::default()),
            admission: Admission::global(),
            prepare: None,
        }
    }

    /// Start every child from this environment rather than this
    /// process's own; the scrub and glab's switches still apply on top.
    pub fn with_environment(mut self, environment: Vec<(OsString, OsString)>) -> Self {
        self.environment = Some(environment);
        self
    }

    /// Other deadlines.
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Another admission than the global one.
    pub fn with_admission(mut self, admission: Arc<Admission>) -> Self {
        self.admission = admission;
        self
    }

    /// How its owner ends it — see [`Cancel`].
    pub fn with_cancel(mut self, cancel: Arc<Cancel>) -> Self {
        self.cancel = cancel;
        self
    }

    /// What each child gets before it starts.
    pub fn with_prepare(mut self, prepare: Prepare) -> Self {
        self.prepare = Some(prepare);
        self
    }

    /// The MR of each branch of one project on one host, **each branch
    /// whole or nothing**: per branch, every page of its MRs (every state,
    /// newest first), the one it maps to, and for an open one its detail,
    /// approvals and every page of its discussions. Only MRs whose source
    /// is this project count — a fork's is not ours (spec review r2,
    /// finding 6).
    ///
    /// A branch's own failure — a call timed out, a record that changed or
    /// is incomplete, any other refusal — is that branch's answer, and the
    /// next branch is still asked. The read fails as a whole only for what
    /// is not a branch's: stopped by its owner, glab gone, the host signed
    /// out (a 401 on any branch asks nothing more), or the deadline spent
    /// before its first branch. A deadline that passes in the middle times
    /// out each branch left without a call.
    pub fn read(&self, host: &str, project: &str, branches: &[String]) -> Result<Answers, Error> {
        self.read_until(
            host,
            project,
            branches,
            Instant::now() + self.limits.refresh,
        )
    }

    /// [`Glab::read`] under a deadline its caller holds — one for a whole
    /// repository's refresh, however many batches it reads (code review r1,
    /// bugs+impl-7).
    pub fn read_until(
        &self,
        host: &str,
        project: &str,
        branches: &[String],
        deadline: Instant,
    ) -> Result<Answers, Error> {
        let project_segment = encode(project);
        let mut answers = Vec::with_capacity(branches.len());
        for branch in branches {
            if self.cancel.is_raised() {
                return Err(Error::Stopped);
            }
            if Instant::now() >= deadline {
                if answers.is_empty() {
                    return Err(Error::TimedOut);
                }
                answers.push((branch.clone(), Err(Error::TimedOut)));
                continue;
            }
            match self.branch(host, &project_segment, branch, deadline) {
                Err(error @ (Error::Stopped | Error::NotInstalled | Error::SignedOut { .. })) => {
                    return Err(error);
                }
                answer => answers.push((branch.clone(), answer)),
            }
        }
        Ok(answers)
    }

    fn branch(
        &self,
        host: &str,
        project: &str,
        branch: &str,
        deadline: Instant,
    ) -> Result<Option<MergeRequest>, Error> {
        let mut listed: Vec<Listed> = Vec::new();
        for page in 1..=PAGES {
            let batch: Vec<Listed> = self.read_json(
                host,
                &format!(
                    "projects/{project}/merge_requests?source_branch={}&state=all&order_by=updated_at&sort=desc&per_page=100&page={page}",
                    encode(branch)
                ),
                deadline,
            )?;
            let last = batch.len() < 100;
            for mr in batch {
                if listed.iter().any(|known| known.iid == mr.iid) {
                    return Err(Error::Changed);
                }
                listed.push(mr);
            }
            if last {
                break;
            }
            if page == PAGES {
                return Err(Error::Failed(
                    "More than 2 000 merge requests from one branch".into(),
                ));
            }
        }
        // A record of this branch that does not say which project it comes
        // from cannot be called a fork's: the answer is unknown, not "no
        // MR" (code review r1, bugs+impl-9).
        // Nor can a record that does not say its source branch be dropped as
        // another branch's (code review r2, bugs+impl-8).
        if listed.iter().any(|mr| mr.source_branch.is_empty()) {
            return Err(Error::Failed(
                "GitLab answered a merge request without its source branch".into(),
            ));
        }
        let mut ours = Vec::new();
        for mr in listed.into_iter().filter(|mr| mr.source_branch == branch) {
            match (mr.source_project_id, mr.project_id) {
                (Some(source), Some(target)) if mr.iid != 0 => {
                    if source == target {
                        ours.push(mr);
                    }
                }
                _ => {
                    return Err(Error::Failed(
                        "GitLab answered a merge request without its project".into(),
                    ));
                }
            }
        }
        let ours = ours.into_iter();
        let Some(chosen) = choose(ours) else {
            return Ok(None);
        };
        // Only a web page is opened from GitLab's answer: anything but an
        // absolute http or https link is an incomplete record (code review
        // r2, adversarial-2).
        if !is_web_link(&chosen.web_url) {
            return Err(Error::Failed(
                "GitLab answered a merge request without a web link".into(),
            ));
        }
        let listed_sha = chosen.sha.clone();
        let mut mr = chosen.into_status();
        if mr.state != State::Open {
            return Ok(Some(mr));
        }
        let base = format!("projects/{project}/merge_requests/{}", mr.iid);
        let detail: Detail = self.read_json(host, &base, deadline)?;
        // Closed or merged between the list and the detail, or GitLab no
        // longer calls it open: not published as open; the next refresh
        // finds it again (code review r1, bugs+impl-10).
        let still_open = matches!(detail.state.as_deref(), Some("opened" | "locked"))
            && detail.detailed_merge_status.as_deref() != Some("not_open");
        if detail.iid != mr.iid || detail.sha != listed_sha || !still_open {
            return Err(Error::Changed);
        }
        let detailed = detail.detailed_merge_status.clone();
        detail.apply(&mut mr);
        mr.approvals = self.approvals(host, &base, deadline)?;
        status::settle_approvals(detailed.as_deref(), &mut mr);
        mr.unresolved_threads = Some(self.unresolved(host, &base, deadline)?);
        Ok(Some(mr))
    }

    /// The id of the account glab is logged in with on `host` — GitLab's
    /// `user` — under the caller's deadline, through the same admission,
    /// caps, environment and preparation as every read. An answer without
    /// an id is an error, never an account.
    pub fn account(&self, host: &str, deadline: Instant) -> Result<u64, Error> {
        if self.cancel.is_raised() {
            return Err(Error::Stopped);
        }
        if Instant::now() >= deadline {
            return Err(Error::TimedOut);
        }
        let account: Account = self.read_json(host, "user", deadline)?;
        Ok(account.id)
    }

    fn approvals(
        &self,
        host: &str,
        base: &str,
        deadline: Instant,
    ) -> Result<Option<Approvals>, Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(default)]
            approved: Option<bool>,
            #[serde(default)]
            approvals_required: Option<u32>,
            #[serde(default)]
            approvals_left: Option<u32>,
            #[serde(default)]
            approved_by: Vec<serde_json::Value>,
        }
        match self.call(host, &format!("{base}/approvals"), deadline) {
            // A tier or a permission without approvals is not a failure of
            // the read: the check is unknown, and earns no ✓.
            Err(Failure::Status(403 | 404, _)) => Ok(None),
            Err(failure) => Err(failure.into_error(host)),
            Ok(value) => {
                let wire: Wire = parse(value)?;
                let given = wire.approved_by.len() as u32;
                // Satisfied only on something GitLab said: its own
                // `approved: true`, nothing left, or what was given against a
                // required count it stated. A GitLab with no approval rule
                // answers `approved: false` with nothing required and nothing
                // left — the person's, 2026-11-25 — and that is satisfied.
                // An answer that says none of these is unknown, not "no
                // approval required" (code review r1, bugs+impl-12).
                let satisfied = if wire.approved == Some(true) || wire.approvals_left == Some(0) {
                    true
                } else if let Some(left) = wire.approvals_left {
                    left == 0
                } else if let Some(required) = wire.approvals_required {
                    given >= required
                } else {
                    return Ok(None);
                };
                Ok(Some(Approvals {
                    given,
                    required: wire.approvals_required.unwrap_or(given),
                    satisfied,
                }))
            }
        }
    }

    fn unresolved(&self, host: &str, base: &str, deadline: Instant) -> Result<u32, Error> {
        let mut seen = std::collections::BTreeSet::new();
        let mut open = 0u32;
        for page in 1..=PAGES {
            let batch: Vec<Discussion> = self.read_json(
                host,
                &format!("{base}/discussions?per_page=100&page={page}"),
                deadline,
            )?;
            let last = batch.len() < 100;
            for discussion in batch {
                if !seen.insert(discussion.id.clone()) {
                    return Err(Error::Changed);
                }
                if discussion.is_open_thread() {
                    open += 1;
                }
            }
            if last {
                return Ok(open);
            }
        }
        Err(Error::Failed("More than 2 000 discussions".into()))
    }

    fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        host: &str,
        endpoint: &str,
        deadline: Instant,
    ) -> Result<T, Error> {
        let value = self
            .call(host, endpoint, deadline)
            .map_err(|failure| failure.into_error(host))?;
        parse(value)
    }

    /// One `glab api`, its answer parsed as JSON.
    fn call(
        &self,
        host: &str,
        endpoint: &str,
        deadline: Instant,
    ) -> Result<serde_json::Value, Failure> {
        let deadline = deadline.min(Instant::now() + self.limits.call);
        // The permit first: no output file exists for a child that waits.
        let _permit = self.admission.take(deadline, &self.cancel)?;
        let io = |e: std::io::Error| Failure::Other(e.to_string());
        let mut command = Command::new(&self.program);
        command
            .arg("api")
            // The status line and headers before the body, on stdout: the
            // status is what says a host is signed out, never a message's
            // words (code review r3, lenses-4). They never reach the
            // diagnostics, which are stderr alone.
            .arg("--include")
            .args(["--hostname", host, endpoint])
            // A neutral folder: no repository's `.git/glab-cli` config is
            // read, and no remote is guessed from.
            .current_dir("/");
        let config_dir = match &self.environment {
            Some(environment) => {
                command.env_clear().envs(environment.iter().cloned());
                environment
                    .iter()
                    .find(|(name, _)| name == "GLAB_CONFIG_DIR")
                    .map(|(_, dir)| dir.clone())
                    .filter(|dir| !dir.is_empty())
                    .map(|dir| absolute_config_dir(&dir))
            }
            None => {
                command.env("PATH", crate::login_path::path_or_inherited());
                config_dir_of_this_process()
            }
        };
        // The child runs in `/`: a relative `GLAB_CONFIG_DIR` would name
        // another folder than the one the login was read from (code review
        // r2, arch+quality-2).
        if let Some(dir) = config_dir {
            command.env("GLAB_CONFIG_DIR", dir);
        }
        for name in SCRUBBED {
            command.env_remove(name);
        }
        command
            .env("GLAB_NO_PROMPT", "true")
            .env("NO_COLOR", "1")
            .env("GLAB_NO_UPDATE_NOTIFIER", "true")
            .env("GLAB_CHECK_UPDATE", "false")
            .env("CHECK_UPDATE", "false")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(prepare) = &self.prepare {
            prepare(&mut command);
        }
        // The last look at the stop and the deadline, under the stop's
        // lock, right before the child.
        // Killable from the quit for as long as this call has it.
        let (mut process, _live) = self.cancel.spawn(&mut command, deadline)?;
        // Each stream is read as it comes, and no byte past its cap is kept
        // — the cap is where Guild captures, not a size looked at later
        // (code review r2, adversarial-5). A stream past its cap ends the
        // child like a deadline.
        let overflow = Arc::new(AtomicBool::new(false));
        let (sent, captured) = mpsc::channel();
        for (stream, cap) in [
            (
                process
                    .take_stdout()
                    .map(|s| Box::new(s) as Box<dyn Read + Send>),
                CAP,
            ),
            (
                process
                    .take_stderr()
                    .map(|s| Box::new(s) as Box<dyn Read + Send>),
                DIAGNOSTICS_CAP,
            ),
        ] {
            let (sent, overflow) = (sent.clone(), overflow.clone());
            let stream = stream.ok_or_else(|| Failure::Other("glab has no output pipe".into()))?;
            std::thread::spawn(move || {
                let _ = sent.send((cap, capture(stream, cap as usize, &overflow)));
            });
        }
        drop(sent);
        // Ending early asks with `SIGTERM` first; `terminate` stops the
        // whole group and reaps its leader. A stop does not end a running
        // child: it runs to its end or its deadline, and its answer is
        // dropped.
        let status = loop {
            if self.cancel.level() == Level::Abort {
                let _ = process.terminate(self.limits.abort_grace);
                return Err(Failure::Stopped);
            }
            if Instant::now() >= deadline {
                let _ = process.terminate(self.limits.grace);
                return Err(Failure::TimedOut);
            }
            if overflow.load(Ordering::Acquire) {
                let _ = process.terminate(self.limits.grace);
                return Err(Failure::TooLarge);
            }
            if let Some(status) = process.try_wait().map_err(io)? {
                break status;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        drop(process);
        // Both streams to their end: what is left in a pipe after the exit.
        let (mut output, mut diagnostics) = (None, None);
        while output.is_none() || diagnostics.is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            match captured.recv_timeout(left) {
                Ok((CAP, bytes)) => output = Some(bytes),
                Ok((_, bytes)) => diagnostics = Some(bytes),
                Err(_) => return Err(Failure::TimedOut),
            }
        }
        if self.cancel.is_raised() {
            return Err(Failure::Stopped);
        }
        if overflow.load(Ordering::Acquire) {
            return Err(Failure::TooLarge);
        }
        let (bytes, diagnostics) = (output.unwrap_or_default(), diagnostics.unwrap_or_default());
        let (http, bytes) = split_response(&bytes);
        if !status.success() {
            return Err(classify(http, &String::from_utf8_lossy(&diagnostics)));
        }
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_slice(bytes).map_err(|e| {
            Failure::Other(format!(
                "glab answered something that is not JSON (line {}, column {})",
                e.line(),
                e.column()
            ))
        })
    }
}

/// Every glab child alive now, by a number of its own, with the right to
/// kill it.
fn live() -> &'static Mutex<Vec<(u64, crate::process::Killer)>> {
    static LIVE: OnceLock<Mutex<Vec<(u64, crate::process::Killer)>>> = OnceLock::new();
    LIVE.get_or_init(Default::default)
}

/// One child's place in [`live`], given up when the call that started it
/// ends. Its group's own reaped flag keeps a kill off a reused ID.
struct Live(u64);

impl Live {
    fn of(process: &ProcessGroup) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        live()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((id, process.killer()));
        Self(id)
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        live()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|(id, _)| *id != self.0);
    }
}

/// `SIGKILL` every glab child still alive, whoever owns it — the quit's
/// last moment, when GPUI will not wait for an owner's grace (code review
/// r2, bugs+impl-1). A group its owner has reaped is not signalled.
pub fn kill_every_child() {
    let killers: Vec<crate::process::Killer> = live()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .map(|(_, killer)| killer.clone())
        .collect();
    crate::process::kill_all(&killers, Duration::from_millis(30));
}

/// A stream read to its end, keeping at most `cap` bytes: past the cap
/// `overflow` is raised and the rest is read and dropped, so the child is
/// never left blocked on a full pipe while it is being ended.
pub fn capture(mut stream: impl Read, cap: usize, overflow: &AtomicBool) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => return kept,
            Ok(n) => {
                let room = cap.saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..n.min(room)]);
                if n > room {
                    overflow.store(true, Ordering::Release);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return kept,
        }
    }
}

/// Whether GitLab's link is an absolute http or https URL with a host,
/// parsed as RFC 3986 parses one: the scheme, `//`, an authority whose
/// userinfo ends at its last `@`, a non-empty host — a name or a bracketed
/// IPv6 address — and an optional numeric port; no whitespace or control
/// character anywhere (code review r3, lenses-7). No URL parser is in this
/// crate's tree, and one is not added for this.
fn is_web_link(url: &str) -> bool {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (host, port) = if let Some(inner) = host_port.strip_prefix('[') {
        match inner.split_once(']') {
            Some((v6, rest))
                if !v6.is_empty()
                    && v6
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.') =>
            {
                (v6, rest.strip_prefix(':'))
            }
            _ => return false,
        }
    } else {
        match host_port.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (host_port, None),
        }
    };
    let host_ok = !host.is_empty()
        && (host_port.starts_with('[')
            || host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~' | '%')));
    let port_ok =
        port.is_none_or(|port| port.is_empty() || port.chars().all(|c| c.is_ascii_digit()));
    host_ok && port_ok
}

/// A `GLAB_CONFIG_DIR` as an absolute path: a relative one is taken from
/// this process's folder, where the login was read from.
pub fn absolute_config_dir(dir: &OsStr) -> PathBuf {
    let dir = Path::new(dir);
    if dir.is_absolute() {
        return dir.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(dir))
        .unwrap_or_else(|_| dir.to_path_buf())
}

/// This process's `GLAB_CONFIG_DIR`, made absolute once — the one the local
/// login read and every glab child use (code review r2, arch+quality-2).
pub fn config_dir_of_this_process() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        std::env::var_os("GLAB_CONFIG_DIR")
            .filter(|dir| !dir.is_empty())
            .map(|dir| absolute_config_dir(&dir))
    })
    .clone()
}

/// The MR a branch maps to: an open one over a merged or closed one, and
/// the most recently updated among those of one state, to the nanosecond;
/// equal times go to the higher number (code review r1, bugs+impl-15).
fn choose(mrs: impl Iterator<Item = Listed>) -> Option<Listed> {
    mrs.max_by_key(|mr| (mr.is_open(), mr.updated(), mr.iid))
}

/// An MR as a list page carries it.
#[derive(Debug, Clone, Deserialize)]
struct Listed {
    iid: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    web_url: String,
    #[serde(default)]
    sha: String,
    #[serde(default)]
    source_branch: String,
    #[serde(default)]
    target_branch: String,
    #[serde(default)]
    source_project_id: Option<u64>,
    #[serde(default)]
    project_id: Option<u64>,
    #[serde(default)]
    state: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    merged_at: Option<String>,
    #[serde(default)]
    closed_at: Option<String>,
    #[serde(default)]
    author: Option<Account>,
}

/// An account as GitLab names one: `author` in a list, `user` itself.
#[derive(Debug, Clone, Deserialize)]
struct Account {
    id: u64,
}

impl Listed {
    fn is_open(&self) -> bool {
        matches!(self.state.as_str(), "opened" | "locked")
    }

    fn updated(&self) -> i128 {
        self.updated_at
            .as_deref()
            .and_then(|text| {
                time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
                    .ok()
            })
            .map_or(0, |at| at.unix_timestamp_nanos())
    }

    fn into_status(self) -> MergeRequest {
        let state = match self.state.as_str() {
            "merged" => State::Merged,
            "closed" => State::Closed,
            _ => State::Open,
        };
        let updated = self.updated_at.as_deref().and_then(seconds);
        // A merged or closed MR GitLab sent no time for still happened, at
        // its last update at the latest.
        let finished = match state {
            State::Merged => self.merged_at.as_deref().and_then(seconds).or(updated),
            State::Closed => self.closed_at.as_deref().and_then(seconds).or(updated),
            State::Open => None,
        };
        MergeRequest {
            iid: self.iid,
            title: self.title,
            web_url: self.web_url,
            state,
            target_branch: self.target_branch,
            draft: self.draft,
            conflict: false,
            mergeable: false,
            needs_rebase: false,
            checking: false,
            block: None,
            approvals: None,
            unresolved_threads: None,
            pipeline: None,
            updated_at: updated,
            finished_at: finished,
            author: self.author.map(|author| author.id),
        }
    }
}

/// An MR's detail: what the list does not carry.
#[derive(Debug, Deserialize)]
struct Detail {
    iid: u64,
    #[serde(default)]
    sha: String,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    target_branch: Option<String>,
    #[serde(default)]
    draft: Option<bool>,
    #[serde(default)]
    has_conflicts: bool,
    #[serde(default)]
    detailed_merge_status: Option<String>,
    #[serde(default)]
    head_pipeline: Option<PipelineWire>,
}

#[derive(Debug, Deserialize)]
struct PipelineWire {
    status: String,
}

impl Detail {
    fn apply(self, mr: &mut MergeRequest) {
        if let Some(title) = self.title.filter(|t| !t.is_empty()) {
            mr.title = title;
        }
        if let Some(target) = self.target_branch.filter(|t| !t.is_empty()) {
            mr.target_branch = target;
        }
        // The detail is read after the list: its word on a draft wins, the
        // list's only stands where it says nothing (code review r2,
        // bugs+impl-13).
        if let Some(draft) = self.draft {
            mr.draft = draft;
        }
        mr.conflict |= self.has_conflicts;
        status::read_detailed_status(self.detailed_merge_status.as_deref(), mr);
        mr.pipeline = self
            .head_pipeline
            .map(|pipeline| status::read_pipeline(&pipeline.status));
        // GitLab requires a pipeline that never ran: a failing line, never a
        // pass (code review r1, bugs+impl-11).
        if mr.pipeline.is_none() && self.detailed_merge_status.as_deref() == Some("ci_must_pass") {
            mr.pipeline = Some(status::Pipeline::Missing);
        }
    }
}

/// A thread: its notes, oldest first.
#[derive(Debug, Deserialize)]
struct Discussion {
    id: String,
    #[serde(default)]
    notes: Vec<Note>,
}

#[derive(Debug, Deserialize)]
struct Note {
    #[serde(default)]
    system: bool,
    #[serde(default)]
    resolvable: bool,
    #[serde(default)]
    resolved: bool,
}

impl Discussion {
    /// A thread a person can resolve and nobody has: its root note.
    fn is_open_thread(&self) -> bool {
        self.notes
            .first()
            .filter(|note| !note.system)
            .is_some_and(|root| root.resolvable && !root.resolved)
    }
}

/// What one call came to, before the caller says what it means.
enum Failure {
    NotInstalled,
    TimedOut,
    Stopped,
    TooLarge,
    Unauthenticated,
    Status(u16, String),
    Other(String),
}

impl Failure {
    fn into_error(self, host: &str) -> Error {
        match self {
            Self::NotInstalled => Error::NotInstalled,
            Self::TimedOut => Error::TimedOut,
            Self::Stopped => Error::Stopped,
            Self::TooLarge => Error::Failed("glab answered more than 8 MB".into()),
            Self::Unauthenticated | Self::Status(401, _) => Error::SignedOut { host: host.into() },
            Self::Status(_, line) | Self::Other(line) => Error::Failed(line),
        }
    }
}

/// The HTTP status and the body of what `glab api --include` printed: the
/// status line and headers, a blank line, then the body. Output without a
/// status line is all body.
fn split_response(bytes: &[u8]) -> (Option<u16>, &[u8]) {
    if !bytes.starts_with(b"HTTP/") {
        return (None, bytes);
    }
    let status = bytes
        .split(|b| *b == b'\n')
        .next()
        .and_then(|line| std::str::from_utf8(line).ok())
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok());
    let body = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4)
        .or_else(|| bytes.windows(2).position(|w| w == b"\n\n").map(|at| at + 2))
        .map_or(&bytes[bytes.len()..], |at| &bytes[at..]);
    (status, body)
}

/// Classify a failed call: by GitLab's HTTP status when there is one —
/// 401 alone is signed out, whatever the message says (code review r3,
/// lenses-4) — else by glab's own words about its login.
fn classify(http: Option<u16>, stderr: &str) -> Failure {
    let lower = stderr.to_ascii_lowercase();
    let line = bounded_line(stderr);
    match http.or_else(|| http_status(&lower)) {
        Some(401) => Failure::Unauthenticated,
        Some(status) => Failure::Status(status, line),
        None if lower.contains("not logged in")
            || lower.contains("glab auth login")
            || lower.contains("unauthenticated")
            || lower.contains("not authenticated") =>
        {
            Failure::Unauthenticated
        }
        None => Failure::Other(line),
    }
}

/// `HTTP nnn` in glab's words.
fn http_status(lower: &str) -> Option<u16> {
    let at = lower.find("http ")? + 5;
    lower.get(at..at + 3)?.parse().ok()
}

/// The first non-empty line, credentials and token-like words removed,
/// at most [`LINE`] characters.
pub fn bounded_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("glab failed");
    let words: Vec<String> = line
        .split(' ')
        .map(|word| {
            let mut word = word.to_owned();
            // userinfo in a URL: scheme://user:secret@host
            if let Some(scheme) = word.find("://")
                && let Some(at) = word[scheme + 3..].find('@')
            {
                word.replace_range(scheme + 3..scheme + 3 + at + 1, "");
            }
            let lower = word.to_ascii_lowercase();
            let tokenish = ["glpat-", "gloas-", "glrt-", "glptt-", "gldt-", "glsoat-"]
                .iter()
                .any(|prefix| lower.contains(prefix))
                || (word.len() >= 24
                    && word
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
            if tokenish { "[…]".into() } else { word }
        })
        .collect();
    let joined = words.join(" ");
    if joined.chars().count() > LINE {
        let mut cut: String = joined.chars().take(LINE - 1).collect();
        cut.push('…');
        cut
    } else {
        joined
    }
}

/// An answer in the shape asked for. A mismatch is said by its kind and
/// never by the value GitLab sent, which serde's own message quotes (code
/// review r1, bugs+impl-4).
fn parse<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, Error> {
    serde_json::from_value(value).map_err(|e| {
        let kind = match e.classify() {
            serde_json::error::Category::Data => "a field of an unexpected type or a missing one",
            serde_json::error::Category::Syntax => "malformed JSON",
            serde_json::error::Category::Eof => "a cut-off answer",
            serde_json::error::Category::Io => "a read error",
        };
        Error::Failed(format!("GitLab answered an unexpected shape: {kind}"))
    })
}

/// One path segment or query value, percent-encoded: everything but
/// RFC 3986's unreserved characters.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// An RFC 3339 time as seconds since the epoch.
fn seconds(text: &str) -> Option<i64> {
    time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|at| at.unix_timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a test holds a child at, between its start and its place in the
    /// live list.
    type Hook = Arc<dyn Fn(u32) + Send + Sync>;
    static AT_SPAWN: Mutex<Option<Hook>> = Mutex::new(None);

    pub(super) fn at_spawn_boundary(pid: u32) {
        let hook = AT_SPAWN.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(hook) = hook {
            hook(pid);
        }
    }

    /// Code review r3, lenses-1: a child started and not yet in the live
    /// list when the quit aborts and kills is killed all the same — the
    /// abort waits for it to be listed, so the kill pass sees it. The child
    /// ignores SIGTERM and would write `survived` after two seconds; the
    /// owner's own grace is ten.
    #[test]
    fn a_child_held_between_start_and_listing_dies_with_the_quit() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("glab");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ntrap '' TERM\nsleep 2\ntouch {}/survived\necho '[]'\n",
                dir.path().display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (held, spawned) = std::sync::mpsc::channel();
        *AT_SPAWN.lock().unwrap() = Some(Arc::new(move |pid| {
            let _ = held.send(pid);
            std::thread::sleep(Duration::from_millis(300));
        }));
        let cancel = Arc::new(Cancel::default());
        let reader = {
            let cancel = cancel.clone();
            let script = script.clone();
            std::thread::spawn(move || {
                Glab::at(script)
                    .with_environment(vec![("PATH".into(), "/usr/bin:/bin".into())])
                    .with_admission(Arc::new(Admission::new(2)))
                    .with_limits(Limits {
                        abort_grace: Duration::from_secs(10),
                        ..Limits::default()
                    })
                    .with_cancel(cancel)
                    .read("gitlab.example.com", "team/app", &["feat".to_owned()])
            })
        };
        spawned
            .recv_timeout(Duration::from_secs(5))
            .expect("the child started");
        // The quit: every refresh aborted, then the last kill pass.
        cancel.abort();
        kill_every_child();
        *AT_SPAWN.lock().unwrap() = None;
        std::thread::sleep(Duration::from_millis(2600));
        let survived = dir.path().join("survived").exists();
        let _ = reader.join();
        assert!(!survived, "the child outlived the quit's kill pass");
    }

    /// Code review r2, adversarial-5: the cap is where the bytes are kept.
    /// Three caps' worth from the child keep one cap — counted — and the
    /// rest is read to its end and dropped.
    #[test]
    fn a_stream_past_its_cap_keeps_the_cap_and_no_more() {
        struct Counted<R> {
            inner: R,
            read: usize,
        }
        impl<R: Read> Read for Counted<R> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = self.inner.read(buf)?;
                self.read += n;
                Ok(n)
            }
        }
        let cap = 100_000;
        let mut stream = Counted {
            inner: std::io::repeat(b'x').take(3 * cap as u64 + 17),
            read: 0,
        };
        let overflow = AtomicBool::new(false);
        let kept = capture(&mut stream, cap, &overflow);
        assert_eq!(kept.len(), cap, "bytes kept");
        assert!(overflow.load(Ordering::Acquire));
        assert_eq!(stream.read, 3 * cap + 17, "drained, not blocked");

        let overflow = AtomicBool::new(false);
        let kept = capture(std::io::repeat(b'y').take(cap as u64), cap, &overflow);
        assert_eq!(kept.len(), cap);
        assert!(!overflow.load(Ordering::Acquire), "exactly the cap fits");
    }

    /// Code review r2, bugs+impl-12: an expired deadline is granted no
    /// permit, even with every slot free.
    #[test]
    fn an_expired_deadline_takes_no_permit() {
        let admission = Arc::new(Admission::new(2));
        let past = Instant::now() - Duration::from_millis(1);
        assert!(matches!(
            admission.take(past, &Cancel::default()),
            Err(Failure::TimedOut)
        ));
        assert!(admission.try_take().is_some() && admission.try_take().is_some());
    }
}
