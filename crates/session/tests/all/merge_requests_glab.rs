//! The glab adapter against a stand-in `glab` — GLD-063, on the base of
//! 69c459cf's reader. No test here runs the real glab or the network.
//!
//! The stand-in answers an endpoint with `<fixtures>/<endpoint>.json`, the
//! endpoint's `/`, `?`, `&`, `=`, `:` and `%` spelled `_`;
//! `<endpoint>.stderr` with `<endpoint>.exit` makes it fail,
//! `<endpoint>.sleep` makes it stall. Every call is logged, one line of
//! argv; the variables named in `ENV_SEEN` are logged as it saw them.
use guild_session::merge_requests::{
    glab::{Admission, Cancel, Error, Glab, Limits},
    hosts::{Hosts, Login},
    remote::{Remote, looks_like_gitlab},
    status::{MergeRequest, Pipeline, State},
};
use std::{
    ffi::OsString,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const SCRIPT: &str = r#"#!/bin/sh
dir="$(dirname "$0")"
[ "$GLAB_NO_PROMPT" = true ] || { echo "would prompt" >&2; exit 3; }
trap 'echo term >> "$dir/signals.log"; exit 143' TERM
endpoint=""
prev=""
for arg in "$@"; do
  case "$prev" in
    --hostname) prev=""; continue ;;
  esac
  case "$arg" in
    api|--include) ;;
    --hostname) prev="$arg" ;;
    *) [ -z "$endpoint" ] && endpoint="$arg" ;;
  esac
done
printf '%s\n' "$*" >> "$dir/calls.log"
for name in $ENV_SEEN; do eval "printf '%s=%s\n' \"\$name\" \"\${$name-<unset>}\"" >> "$dir/env.log"; done
mkdir "$dir/running/$$" 2>/dev/null
n=$(ls "$dir/running" | wc -l | tr -d ' ')
echo "$n" >> "$dir/concurrency.log"
name="$(printf '%s' "$endpoint" | tr '/?&=:%' '______')"
# The whole group deaf to SIGTERM: only a SIGKILL ends it.
if [ -f "$dir/fixtures/$name.deaf" ]; then trap '' TERM; fi
if [ -f "$dir/fixtures/$name.sleep" ]; then sleep "$(cat "$dir/fixtures/$name.sleep")"; fi
echo "$endpoint" >> "$dir/finished.log"
rmdir "$dir/running/$$"
# What glab prints with --include: the status line and headers, then the body.
if [ -f "$dir/fixtures/$name.raw" ]; then
  cat "$dir/fixtures/$name.raw"
  [ -f "$dir/fixtures/$name.stderr" ] || exit 0
fi
if [ -f "$dir/fixtures/$name.stderr" ]; then
  cat "$dir/fixtures/$name.stderr" >&2
  exit "$(cat "$dir/fixtures/$name.exit" 2>/dev/null || echo 1)"
fi
if [ -f "$dir/fixtures/$name.flood" ]; then
  yes xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx >&"$(cat "$dir/fixtures/$name.flood")"
fi
if [ -f "$dir/fixtures/$name.json" ]; then cat "$dir/fixtures/$name.json"; exit 0; fi
echo "glab: 404 Not Found (HTTP 404)" >&2
exit 1
"#;

struct Stand {
    dir: tempfile::TempDir,
}

const HOST: &str = "gitlab.example.com";
const PROJECT: &str = "team/app";
/// `team/app` as one path segment.
const ENC: &str = "team%2Fapp";

impl Stand {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("fixtures")).unwrap();
        std::fs::create_dir(dir.path().join("running")).unwrap();
        let script = dir.path().join("glab");
        std::fs::write(&script, SCRIPT).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        // macOS checks a new executable at its first exec; while the suite
        // starts dozens at once that took over 2 s — longer than the
        // deadlines below. Run it once, before any test's clock: without
        // `GLAB_NO_PROMPT` it exits at its third line, logging nothing.
        let warm = std::process::Command::new(&script)
            .env_clear()
            .current_dir("/")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(warm.code(), Some(3), "the stand-in refused to prompt");
        Self { dir }
    }
    fn name(endpoint: &str) -> String {
        endpoint
            .chars()
            .map(|c| if "/?&=:%".contains(c) { '_' } else { c })
            .collect()
    }
    fn fixture(&self, endpoint: &str, ext: &str) -> PathBuf {
        self.dir
            .path()
            .join("fixtures")
            .join(format!("{}.{ext}", Self::name(endpoint)))
    }
    fn answer(&self, endpoint: &str, json: serde_json::Value) {
        std::fs::write(self.fixture(endpoint, "json"), json.to_string()).unwrap();
    }
    fn fail(&self, endpoint: &str, stderr: &str) {
        std::fs::write(self.fixture(endpoint, "stderr"), stderr).unwrap();
        std::fs::write(self.fixture(endpoint, "exit"), "1").unwrap();
    }
    fn stall(&self, endpoint: &str, seconds: f32) {
        std::fs::write(self.fixture(endpoint, "sleep"), seconds.to_string()).unwrap();
    }
    /// glab's own output for a response, as `--include` prints it.
    fn raw(&self, endpoint: &str, status: &str, body: &str, stderr: Option<&str>) {
        std::fs::write(
            self.fixture(endpoint, "raw"),
            format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nX-Request-Id: abc\r\n\r\n{body}"),
        )
        .unwrap();
        if let Some(stderr) = stderr {
            self.fail(endpoint, stderr);
        }
    }
    fn deaf(&self, endpoint: &str) {
        std::fs::write(self.fixture(endpoint, "deaf"), "").unwrap();
    }
    fn flood(&self, endpoint: &str, fd: u8) {
        std::fs::write(self.fixture(endpoint, "flood"), fd.to_string()).unwrap();
    }
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn env_seen(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("env.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn log(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(name)).unwrap_or_default()
    }
    fn max_concurrency(&self) -> usize {
        std::fs::read_to_string(self.dir.path().join("concurrency.log"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .max()
            .unwrap_or(0)
    }
    fn program(&self) -> PathBuf {
        self.dir.path().join("glab")
    }
    /// A bare environment: what the child starts from, before the adapter
    /// removes and sets its variables.
    fn base_env() -> Vec<(OsString, OsString)> {
        vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            // A home with nothing in it: no glab config, no keyring.
            ("HOME".into(), "/nonexistent-gld-063-home".into()),
        ]
    }
    fn glab(&self) -> Glab {
        Glab::at(self.program())
            .with_environment(Self::base_env())
            .with_admission(Arc::new(Admission::new(2)))
    }

    fn list(&self, branch: &str, page: u32) -> String {
        format!(
            "projects/{ENC}/merge_requests?source_branch={branch}&state=all&order_by=updated_at&sort=desc&per_page=100&page={page}"
        )
    }
    fn base(iid: u64) -> String {
        format!("projects/{ENC}/merge_requests/{iid}")
    }

    /// One open MR on `branch`, read whole: listed, its detail, approvals
    /// and one page of discussions.
    fn open(&self, branch: &str, iid: u64) {
        self.answer(
            &self.list(branch, 1),
            serde_json::json!([listed(iid, branch, "opened")]),
        );
        let mut detail = listed(iid, branch, "opened");
        detail["detailed_merge_status"] = "mergeable".into();
        detail["head_pipeline"] = serde_json::json!({"status": "success"});
        self.answer(&Self::base(iid), detail);
        self.answer(
            &format!("{}/approvals", Self::base(iid)),
            serde_json::json!({"approved": true, "approvals_required": 1, "approvals_left": 0,
                "approved_by": [{"user": {"id": 1, "username": "a"}}]}),
        );
        self.answer(
            &format!("{}/discussions?per_page=100&page=1", Self::base(iid)),
            serde_json::json!([]),
        );
    }
}

fn listed(iid: u64, branch: &str, state: &str) -> serde_json::Value {
    serde_json::json!({
        "iid": iid,
        "title": format!("Change {iid}"),
        "web_url": format!("https://{HOST}/{PROJECT}/-/merge_requests/{iid}"),
        "sha": format!("head{iid}"),
        "source_branch": branch,
        "target_branch": "main",
        "source_project_id": 7,
        "project_id": 7,
        "state": state,
        "updated_at": "2026-11-20T10:00:00.000Z",
    })
}

fn thread(id: &str, resolved: bool) -> serde_json::Value {
    serde_json::json!({"id": id, "notes": [{
        "id": 1, "body": "why?", "system": false, "resolvable": true, "resolved": resolved
    }]})
}

fn branches(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

// --- reading

#[test]
fn an_open_mr_is_read_whole_with_the_host_pinned() {
    let stand = Stand::new();
    stand.open("feat", 294);
    stand.answer(
        &format!("{}/discussions?per_page=100&page=1", Stand::base(294)),
        serde_json::json!([thread("d1", false), thread("d2", true), thread("d3", false)]),
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    assert_eq!(read.len(), 1);
    let (branch, mr) = &read[0];
    assert_eq!(branch, "feat");
    let mr = mr.as_ref().expect("an MR");
    assert_eq!(mr.iid, 294);
    assert_eq!(mr.state, State::Open);
    assert!(mr.mergeable);
    assert_eq!(mr.pipeline, Some(Pipeline::Passed));
    assert_eq!(mr.unresolved_threads, Some(2));
    let approvals = mr.approvals.unwrap();
    assert_eq!((approvals.given, approvals.required), (1, 1));
    assert!(approvals.satisfied);
    assert_eq!(mr.target_branch, "main");
    assert!(mr.updated_at.is_some());
    let calls = stand.calls();
    assert!(
        calls
            .iter()
            .all(|call| call
                .starts_with(&format!("api --include --hostname {HOST} projects/{ENC}/"))),
        "{calls:#?}"
    );
}

#[test]
fn a_branch_with_no_mr_answers_none() {
    let stand = Stand::new();
    stand.answer(&stand.list("lonely", 1), serde_json::json!([]));
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["lonely"]))
        .and_then(whole)
        .unwrap();
    assert_eq!(read, vec![("lonely".to_owned(), None)]);
}

#[test]
fn an_open_mr_wins_over_a_merged_one_and_forks_are_not_ours() {
    let stand = Stand::new();
    let mut fork = listed(5, "feat", "opened");
    fork["source_project_id"] = 99.into();
    let mut newer_merged = listed(4, "feat", "merged");
    newer_merged["updated_at"] = "2026-11-24T10:00:00Z".into();
    let mut other_branch = listed(6, "feat-2", "opened");
    other_branch["updated_at"] = "2026-11-25T10:00:00Z".into();
    stand.answer(
        &stand.list("feat", 1),
        serde_json::json!([
            fork,
            newer_merged,
            other_branch,
            listed(3, "feat", "opened")
        ]),
    );
    let mut detail = listed(3, "feat", "opened");
    detail["detailed_merge_status"] = "not_approved".into();
    stand.answer(&Stand::base(3), detail);
    stand.answer(
        &format!("{}/approvals", Stand::base(3)),
        serde_json::json!({"approved": false, "approvals_required": 1, "approvals_left": 1, "approved_by": []}),
    );
    stand.answer(
        &format!("{}/discussions?per_page=100&page=1", Stand::base(3)),
        serde_json::json!([]),
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    let mr = read[0].1.as_ref().unwrap();
    assert_eq!(
        mr.iid, 3,
        "the open one, not the newer merged or the fork's"
    );
    assert!(!mr.mergeable);
}

#[test]
fn a_merged_mr_needs_no_detail() {
    let stand = Stand::new();
    let mut merged = listed(9, "done", "merged");
    merged["merged_at"] = "2026-11-21T09:00:00Z".into();
    stand.answer(&stand.list("done", 1), serde_json::json!([merged]));
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["done"]))
        .and_then(whole)
        .unwrap();
    let mr = read[0].1.as_ref().unwrap();
    assert_eq!(mr.state, State::Merged);
    assert!(mr.finished_at.is_some());
    assert_eq!(stand.calls().len(), 1, "{:#?}", stand.calls());
}

#[test]
fn a_branch_name_is_one_query_value() {
    let stand = Stand::new();
    stand.answer(&stand.list("feature%2Fx%26y", 1), serde_json::json!([]));
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feature/x&y"]))
        .and_then(whole)
        .unwrap();
    assert_eq!(read, vec![("feature/x&y".to_owned(), None)]);
}

#[test]
fn every_page_of_mrs_and_discussions_is_read() {
    let stand = Stand::new();
    stand.open("feat", 1);
    let filler: Vec<serde_json::Value> = (100..200)
        .map(|iid| {
            let mut mr = listed(iid, "feat", "closed");
            mr["updated_at"] = "2026-01-01T00:00:00Z".into();
            mr
        })
        .collect();
    stand.answer(&stand.list("feat", 1), serde_json::Value::Array(filler));
    stand.answer(
        &stand.list("feat", 2),
        serde_json::json!([listed(1, "feat", "opened")]),
    );
    let page = |from: u32, count: u32, resolved: bool| {
        serde_json::Value::Array(
            (from..from + count)
                .map(|n| thread(&format!("d{n}"), resolved))
                .collect(),
        )
    };
    stand.answer(
        &format!("{}/discussions?per_page=100&page=1", Stand::base(1)),
        page(0, 100, true),
    );
    stand.answer(
        &format!("{}/discussions?per_page=100&page=2", Stand::base(1)),
        page(100, 3, false),
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    let mr = read[0].1.as_ref().unwrap();
    assert_eq!(mr.iid, 1, "the open MR was on the second page");
    assert_eq!(
        mr.unresolved_threads,
        Some(3),
        "the second page's threads count"
    );
}

/// A read whose every branch answered: the first branch's failure is the
/// read's. What each test of one branch's reading asks; a test of how the
/// branches stand apart reads the answers as they come.
fn whole(
    read: Vec<(String, Result<Option<MergeRequest>, Error>)>,
) -> Result<Vec<(String, Option<MergeRequest>)>, Error> {
    read.into_iter()
        .map(|(branch, mr)| mr.map(|mr| (branch, mr)))
        .collect()
}

/// A failed page fails its own branch, whole: the branches before and after
/// it in the same project keep their answers, and the one after is still
/// asked.
#[test]
fn a_failed_page_fails_its_branch_and_no_other() {
    let stand = Stand::new();
    stand.open("a", 1);
    stand.open("b", 2);
    stand.open("c", 3);
    stand.fail(
        &format!("{}/discussions?per_page=100&page=1", Stand::base(2)),
        "glab: 500 Internal Server Error (HTTP 500)\n",
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["a", "b", "c"]))
        .unwrap();
    assert_eq!(read.len(), 3);
    assert_eq!(read[0].0, "a");
    assert_eq!(read[0].1.as_ref().unwrap().as_ref().unwrap().iid, 1);
    assert_eq!(read[1].0, "b");
    assert!(matches!(read[1].1, Err(Error::Failed(_))), "{:?}", read[1]);
    assert_eq!(read[2].0, "c");
    assert_eq!(read[2].1.as_ref().unwrap().as_ref().unwrap().iid, 3);
    assert!(
        stand.calls().iter().any(|c| c.contains("source_branch=c")),
        "the branch after the failed one is asked"
    );
}

/// The deadline passing in the middle of a batch times out the branch it
/// caught and each one after it, without a call; what was read before it
/// stands.
#[test]
fn a_deadline_mid_batch_times_out_only_the_rest() {
    let stand = Stand::new();
    stand.answer(&stand.list("a", 1), serde_json::json!([]));
    stand.answer(&stand.list("b", 1), serde_json::json!([]));
    stand.stall(&stand.list("b", 1), 30.0);
    stand.answer(&stand.list("c", 1), serde_json::json!([]));
    let glab = stand.glab().with_limits(Limits {
        call: Duration::from_secs(20),
        refresh: Duration::from_secs(20),
        ..Limits::default()
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let read = glab
        .read_until(HOST, PROJECT, &branches(&["a", "b", "c"]), deadline)
        .unwrap();
    assert_eq!(read[0], ("a".to_owned(), Ok(None)));
    assert_eq!(read[1], ("b".to_owned(), Err(Error::TimedOut)));
    assert_eq!(read[2], ("c".to_owned(), Err(Error::TimedOut)));
    assert!(!stand.calls().iter().any(|c| c.contains("source_branch=c")));
}

/// A 401 on any branch is the host's, not the branch's: the read fails as
/// a whole and asks nothing more.
#[test]
fn a_401_on_one_branch_is_the_whole_read_s() {
    let stand = Stand::new();
    stand.answer(&stand.list("a", 1), serde_json::json!([]));
    stand.fail(&stand.list("b", 1), "glab: 401 Unauthorized (HTTP 401)\n");
    stand.answer(&stand.list("c", 1), serde_json::json!([]));
    assert_eq!(
        stand
            .glab()
            .read(HOST, PROJECT, &branches(&["a", "b", "c"]))
            .unwrap_err(),
        Error::SignedOut { host: HOST.into() }
    );
    assert!(!stand.calls().iter().any(|c| c.contains("source_branch=c")));
}

/// GitLab with no approval rule answers `approved: false` with nothing
/// required and no `approvals_left` — seen on the person's GitLab on
/// 2026-11-25. Nothing required is nothing in the way.
#[test]
fn no_approval_required_is_satisfied() {
    let stand = Stand::new();
    stand.open("feat", 1);
    stand.answer(
        &format!("{}/approvals", Stand::base(1)),
        serde_json::json!({"approved": false, "approvals_required": 0, "approved_by": []}),
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    let approvals = read[0].1.as_ref().unwrap().approvals.unwrap();
    assert_eq!((approvals.given, approvals.required), (0, 0));
    assert!(approvals.satisfied, "{approvals:?}");
    // Still not satisfied while GitLab says one is left.
    stand.answer(
        &format!("{}/approvals", Stand::base(1)),
        serde_json::json!({"approved": false, "approvals_required": 1, "approvals_left": 1, "approved_by": []}),
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    assert!(!read[0].1.as_ref().unwrap().approvals.unwrap().satisfied);
}

#[test]
fn approvals_gitlab_does_not_offer_are_unknown_not_a_failure() {
    let stand = Stand::new();
    stand.open("feat", 1);
    std::fs::remove_file(stand.fixture(&format!("{}/approvals", Stand::base(1)), "json")).unwrap();
    stand.fail(
        &format!("{}/approvals", Stand::base(1)),
        "glab: 403 Forbidden (HTTP 403)\n",
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    assert_eq!(read[0].1.as_ref().unwrap().approvals, None);
}

#[test]
fn a_head_that_moves_between_the_list_and_the_detail_is_no_answer() {
    let stand = Stand::new();
    stand.open("feat", 1);
    let mut detail = listed(1, "feat", "opened");
    detail["sha"] = "moved".into();
    stand.answer(&Stand::base(1), detail);
    let error = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap_err();
    assert_eq!(error, Error::Changed);
}

// --- bounds

#[test]
fn a_stalled_glab_is_stopped_at_its_deadline() {
    let stand = Stand::new();
    stand.open("feat", 1);
    stand.stall(&stand.list("feat", 1), 30.0);
    let started = Instant::now();
    let error = stand
        .glab()
        .with_limits(Limits {
            call: Duration::from_millis(300),
            refresh: Duration::from_secs(5),
            ..Limits::default()
        })
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap_err();
    assert_eq!(error, Error::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn output_past_its_cap_stops_the_child_while_it_runs() {
    for fd in [1u8, 2] {
        let stand = Stand::new();
        let endpoint = stand.list("feat", 1);
        stand.flood(&endpoint, fd);
        let started = Instant::now();
        let error = stand
            .glab()
            .with_limits(Limits {
                call: Duration::from_secs(20),
                refresh: Duration::from_secs(20),
                ..Limits::default()
            })
            .read(HOST, PROJECT, &branches(&["feat"]))
            .and_then(whole)
            .unwrap_err();
        assert!(
            matches!(error, Error::Failed(ref why) if why.contains("MB")),
            "fd {fd}: {error:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "fd {fd}: stopped while it ran, not at the deadline"
        );
    }
}

#[test]
fn no_more_children_than_the_admission_allows_across_reads() {
    let stand = Stand::new();
    let admission = Arc::new(Admission::new(2));
    for i in 0..6 {
        let branch = format!("b{i}");
        stand.answer(&stand.list(&branch, 1), serde_json::json!([]));
        stand.stall(&stand.list(&branch, 1), 0.3);
    }
    std::thread::scope(|scope| {
        for i in 0..6 {
            let glab = stand.glab().with_admission(admission.clone());
            scope.spawn(move || {
                glab.read(HOST, PROJECT, &branches(&[&format!("b{i}")]))
                    .and_then(whole)
                    .unwrap();
            });
        }
    });
    assert_eq!(stand.calls().len(), 6);
    assert!(
        stand.max_concurrency() <= 2,
        "{} at once",
        stand.max_concurrency()
    );
}

#[test]
fn a_read_waiting_for_admission_still_has_a_deadline() {
    let stand = Stand::new();
    let admission = Arc::new(Admission::new(1));
    let held = admission.try_take().expect("the only permit");
    stand.answer(&stand.list("feat", 1), serde_json::json!([]));
    let started = Instant::now();
    let error = stand
        .glab()
        .with_admission(admission.clone())
        .with_limits(Limits {
            call: Duration::from_millis(200),
            refresh: Duration::from_millis(400),
            ..Limits::default()
        })
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap_err();
    assert_eq!(error, Error::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(stand.calls().is_empty(), "no child before a permit");
    drop(held);
}

// --- the child's environment

/// Named here, not taken from the adapter's own list, so that a name
/// dropped from it fails this test (code review r1, arch+quality-2).
const OVERRIDES: &[&str] = &[
    // endpoint and base path
    "GITLAB_HOST",
    "GITLAB_URI",
    "GITLAB_URL",
    "GL_HOST",
    "GITLAB_API_HOST",
    "GITLAB_API_URL",
    "GLAB_API_PROTOCOL",
    "API_PROTOCOL",
    "GITLAB_SUBFOLDER",
    "CI_SERVER_URL",
    // glab's own proxy, which wins over the environment's
    "GLAB_PROXY",
    "PROXY",
    // TLS
    "GLAB_CA_CERT",
    "CA_CERT",
    "GLAB_SKIP_TLS_VERIFY",
    "SKIP_TLS_VERIFY",
    // credentials bound to no host
    "GITLAB_TOKEN",
    "GITLAB_ACCESS_TOKEN",
    "OAUTH_TOKEN",
    "GLAB_IS_OAUTH2",
    "CI_JOB_TOKEN",
    // debug output
    "DEBUG",
    "GLAB_DEBUG",
    "GLAB_DEBUG_HTTP",
];

#[test]
fn the_child_never_sees_endpoint_proxy_or_credential_overrides() {
    let stand = Stand::new();
    stand.answer(&stand.list("feat", 1), serde_json::json!([]));
    let mut env = Stand::base_env();
    for name in OVERRIDES {
        env.push(((*name).into(), "set-by-the-parent".into()));
    }
    env.push(("ENV_SEEN".into(), OVERRIDES.join(" ").into()));
    stand
        .glab()
        .with_environment(env)
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    let seen = stand.env_seen();
    assert_eq!(seen.len(), OVERRIDES.len(), "{seen:#?}");
    for line in &seen {
        assert!(line.ends_with("=<unset>"), "{line}");
    }
}

#[test]
fn the_config_dir_and_the_prepared_proxy_pass_through() {
    let stand = Stand::new();
    stand.answer(&stand.list("feat", 1), serde_json::json!([]));
    let mut env = Stand::base_env();
    env.push(("GLAB_CONFIG_DIR".into(), "/somewhere".into()));
    env.push((
        "ENV_SEEN".into(),
        "GLAB_CONFIG_DIR HTTPS_PROXY NO_COLOR".into(),
    ));
    stand
        .glab()
        .with_environment(env)
        .with_prepare(Arc::new(|command: &mut std::process::Command| {
            command.env("HTTPS_PROXY", "http://proxy.example:3128");
        }))
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    assert_eq!(
        stand.env_seen(),
        vec![
            "GLAB_CONFIG_DIR=/somewhere",
            "HTTPS_PROXY=http://proxy.example:3128",
            "NO_COLOR=1",
        ]
    );
}

// --- failures

#[test]
fn glab_not_on_the_login_path_is_not_installed() {
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(
        Glab::locate(&empty.path().display().to_string()).unwrap_err(),
        Error::NotInstalled
    );
    let stand = Stand::new();
    let found = Glab::locate(&format!("relative:{}", stand.dir.path().display())).unwrap();
    assert_eq!(found, stand.program().canonicalize().unwrap());
}

#[test]
fn unauthorized_is_signed_out_naming_the_host() {
    let stand = Stand::new();
    stand.fail(
        &stand.list("feat", 1),
        "glab: 401 Unauthorized (HTTP 401)\n",
    );
    assert_eq!(
        stand
            .glab()
            .read(HOST, PROJECT, &branches(&["feat"]))
            .and_then(whole)
            .unwrap_err(),
        Error::SignedOut { host: HOST.into() }
    );
}

#[test]
fn other_failures_are_one_bounded_line_without_token_like_text() {
    let stand = Stand::new();
    stand.fail(
        &stand.list("feat", 1),
        &format!(
            // Assembled, so the repository holds no token-shaped literal.
            "glab: request to https://oauth2:{}@{HOST} failed {} {}\nsecond line\n",
            ["gl", "pat-", "fake"].concat(),
            ["gl", "pat-", &"Ab".repeat(10)].concat(),
            "y".repeat(400)
        ),
    );
    let Error::Failed(line) = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap_err()
    else {
        panic!("not a failure")
    };
    assert!(!line.contains("glpat"), "{line}");
    assert!(!line.contains("oauth2:"), "{line}");
    assert!(!line.contains("second line"), "{line}");
    assert!(line.chars().count() <= 200, "{}", line.len());
}

// --- remotes

#[test]
fn a_remote_url_is_a_host_and_a_project_path() {
    let cases = [
        (
            "https://gitlab.example.com/team/app.git",
            "gitlab.example.com",
            "team/app",
        ),
        (
            "https://gitlab.example.com/team/sub/app",
            "gitlab.example.com",
            "team/sub/app",
        ),
        (
            "https://user:secret@gitlab.example.com:8443/team/app.git/",
            "gitlab.example.com:8443",
            "team/app",
        ),
        (
            "ssh://git@gitlab.example.com:2222/team/app.git",
            "gitlab.example.com",
            "team/app",
        ),
        (
            "git@gitlab.example.com:team/app.git",
            "gitlab.example.com",
            "team/app",
        ),
        ("gitlab-work:team/app", "gitlab-work", "team/app"),
    ];
    for (url, host, project) in cases {
        assert_eq!(
            Remote::parse(url),
            Some(Remote {
                host: host.into(),
                project: project.into(),
                ssh: !url.starts_with("http"),
            }),
            "{url}"
        );
    }
    for url in ["/srv/repos/app.git", "file:///srv/app", "", "https://host/"] {
        assert_eq!(Remote::parse(url), None, "{url}");
    }
}

#[test]
fn a_host_looks_like_gitlab_by_a_label() {
    assert!(looks_like_gitlab("gitlab.example.com"));
    assert!(looks_like_gitlab("my-gitlab.corp"));
    assert!(looks_like_gitlab("gitlab.com"));
    assert!(!looks_like_gitlab("github.com"));
    assert!(!looks_like_gitlab("code.example.org"));
}

// --- glab's hosts, read locally

const CONFIG: &str = "git_protocol: ssh
host: gitlab.acme.example
hosts:
    gitlab.com:
        api_protocol: https
        api_host: gitlab.com
        token:
        user:
    signed.example:
        token: present
        api_host: signed.example
        ssh_host: ssh.signed.example
    keyring.example:
        use_keyring: true
        api_host: api.keyring.example
        user: someone
    named.only.example:
        user: someone
";

#[test]
fn glab_hosts_are_read_from_its_config_without_the_network() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.yml"), CONFIG).unwrap();
    let hosts = Hosts::read(&dir.path().join("config.yml"));
    assert_eq!(
        hosts.login("signed.example", true),
        Login::SignedIn("signed.example".into())
    );
    assert_eq!(
        hosts.login("ssh.signed.example", true),
        Login::SignedIn("signed.example".into()),
        "an ssh host names its glab host"
    );
    assert_eq!(
        hosts.login("keyring.example", true),
        Login::SignedIn("keyring.example".into())
    );
    assert_eq!(hosts.login("named.only.example", true), Login::SignedOut);
    assert_eq!(hosts.login("gitlab.com", true), Login::SignedOut);
    assert_eq!(hosts.login("github.com", true), Login::Unknown);
}

// --- code review r1

/// bugs+impl-9: a listed record that does not say which project it is
/// from cannot be called "not ours"; the batch is unknown, not "no MR".
#[test]
fn an_answer_without_its_project_is_unknown_not_none() {
    let stand = Stand::new();
    let mut unsaid = listed(1, "feat", "opened");
    unsaid.as_object_mut().unwrap().remove("project_id");
    stand.answer(&stand.list("feat", 1), serde_json::json!([unsaid]));
    let error = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap_err();
    assert!(matches!(error, Error::Failed(_)), "{error:?}");
}

/// bugs+impl-10: closed between the list and the detail, the MR is not
/// published as open; the next refresh picks it again.
#[test]
fn an_mr_closed_between_list_and_detail_is_no_answer() {
    for (state, status) in [
        ("closed", "not_open"),
        ("merged", "mergeable"),
        ("opened", "not_open"),
    ] {
        let stand = Stand::new();
        stand.open("feat", 1);
        let mut detail = listed(1, "feat", state);
        detail["detailed_merge_status"] = status.into();
        stand.answer(&Stand::base(1), detail);
        let error = stand
            .glab()
            .read(HOST, PROJECT, &branches(&["feat"]))
            .and_then(whole)
            .unwrap_err();
        assert_eq!(error, Error::Changed, "{state} {status}");
    }
}

/// bugs+impl-11: GitLab requires a pipeline that never ran — a failing
/// line and no ✓, never a pass.
#[test]
fn ci_must_pass_without_a_pipeline_is_not_a_check() {
    use guild_session::merge_requests::status::{Pipeline, badge, first_failing};
    let stand = Stand::new();
    stand.open("feat", 1);
    let mut detail = listed(1, "feat", "opened");
    detail["detailed_merge_status"] = "ci_must_pass".into();
    stand.answer(&Stand::base(1), detail);
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    let mr = read[0].1.as_ref().unwrap();
    assert_eq!(mr.pipeline, Some(Pipeline::Missing));
    assert_eq!(badge(mr), None);
    assert!(first_failing(mr).is_some());
}

/// bugs+impl-12: an approvals answer that says nothing is unknown — no
/// line and no ✓ — while 4f339d4d's real answer stays satisfied.
#[test]
fn approvals_missing_their_fields_are_unknown() {
    for answer in [
        serde_json::json!({}),
        serde_json::json!({"approved_by": []}),
    ] {
        let stand = Stand::new();
        stand.open("feat", 1);
        stand.answer(&format!("{}/approvals", Stand::base(1)), answer.clone());
        let read = stand
            .glab()
            .read(HOST, PROJECT, &branches(&["feat"]))
            .and_then(whole)
            .unwrap();
        assert_eq!(read[0].1.as_ref().unwrap().approvals, None, "{answer}");
    }
}

/// bugs+impl-4: a malformed answer is said by its shape and place, never
/// by the value GitLab sent.
#[test]
fn a_malformed_answer_says_its_shape_not_its_values() {
    let stand = Stand::new();
    stand.open("feat", 1);
    stand.answer(
        &format!("{}/approvals", Stand::base(1)),
        serde_json::json!({"approved": false, "approvals_required": "a-private-value"}),
    );
    let Error::Failed(line) = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap_err()
    else {
        panic!("not a failure")
    };
    assert!(!line.contains("a-private-value"), "{line}");
    assert!(line.chars().count() <= 200, "{line}");
}

/// bugs+impl-15: within one second the newer of two open MRs wins.
#[test]
fn the_newest_mr_wins_within_a_second() {
    let stand = Stand::new();
    let mut newer = listed(7, "feat", "opened");
    newer["updated_at"] = "2026-11-20T10:00:00.900Z".into();
    let mut older = listed(8, "feat", "opened");
    older["updated_at"] = "2026-11-20T10:00:00.100Z".into();
    stand.answer(&stand.list("feat", 1), serde_json::json!([newer, older]));
    stand.open("feat", 7);
    stand.answer(&stand.list("feat", 1), serde_json::json!([newer, older]));
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    assert_eq!(read[0].1.as_ref().unwrap().iid, 7);
}

/// bugs+impl-7: one deadline for a repository's whole refresh — a second
/// batch read under the same deadline does not start its own clock.
#[test]
fn one_deadline_bounds_every_batch() {
    let stand = Stand::new();
    stand.answer(&stand.list("a", 1), serde_json::json!([]));
    stand.stall(&stand.list("a", 1), 30.0);
    stand.answer(&stand.list("b", 1), serde_json::json!([]));
    let glab = stand.glab().with_limits(Limits {
        call: Duration::from_secs(20),
        refresh: Duration::from_secs(20),
        ..Limits::default()
    });
    let deadline = Instant::now() + Duration::from_millis(600);
    let started = Instant::now();
    assert_eq!(
        glab.read_until(HOST, PROJECT, &branches(&["a"]), deadline)
            .and_then(whole)
            .unwrap_err(),
        Error::TimedOut
    );
    assert_eq!(
        glab.read_until(HOST, PROJECT, &branches(&["b"]), deadline)
            .and_then(whole)
            .unwrap_err(),
        Error::TimedOut,
        "the second batch has no time left"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(!stand.calls().iter().any(|c| c.contains("source_branch=b")));
}

/// docs+tests-3: a stop raised while a call is being prepared starts no
/// child at all.
#[test]
fn a_stop_before_spawn_starts_no_child() {
    let stand = Stand::new();
    stand.answer(&stand.list("feat", 1), serde_json::json!([]));
    let cancel = Arc::new(Cancel::default());
    let raised = cancel.clone();
    let error = stand
        .glab()
        .with_cancel(cancel)
        .with_prepare(Arc::new(move |_| raised.stop()))
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap_err();
    assert_eq!(error, Error::Stopped);
    assert!(stand.calls().is_empty(), "{:?}", stand.calls());
}

/// arch+quality-1: a stop (the setting off, a repository closed) lets a
/// child already running finish — glab may be saving a refreshed login —
/// and drops its answer; nothing more is started.
#[test]
fn a_stop_after_spawn_lets_the_child_finish_and_drops_its_answer() {
    let stand = Stand::new();
    stand.answer(&stand.list("a", 1), serde_json::json!([]));
    stand.stall(&stand.list("a", 1), 1.0);
    stand.answer(&stand.list("b", 1), serde_json::json!([]));
    let cancel = Arc::new(Cancel::default());
    let raised = cancel.clone();
    let calls = stand.dir.path().join("calls.log");
    std::thread::spawn(move || {
        while !calls.exists() {
            std::thread::sleep(Duration::from_millis(10));
        }
        raised.stop();
    });
    let error = stand
        .glab()
        .with_cancel(cancel)
        .read(HOST, PROJECT, &branches(&["a", "b"]))
        .and_then(whole)
        .unwrap_err();
    assert_eq!(error, Error::Stopped);
    assert_eq!(stand.log("signals.log"), "", "not signalled");
    assert!(
        stand.log("finished.log").contains("source_branch=a"),
        "ran to its end"
    );
    assert!(!stand.calls().iter().any(|c| c.contains("source_branch=b")));
}

/// arch+quality-1: the deadline asks with SIGTERM first, so glab can
/// finish saving, and kills only after a grace.
#[test]
fn the_deadline_terminates_before_it_kills() {
    let stand = Stand::new();
    stand.answer(&stand.list("feat", 1), serde_json::json!([]));
    stand.stall(&stand.list("feat", 1), 30.0);
    // Long enough for the stand-in to have set its trap, which it does
    // before it logs the call.
    let error = stand
        .glab()
        .with_limits(Limits {
            call: Duration::from_secs(5),
            refresh: Duration::from_secs(10),
            grace: Duration::from_secs(10),
            ..Limits::default()
        })
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap_err();
    assert_eq!(error, Error::TimedOut);
    assert!(!stand.calls().is_empty(), "the child ran");
    assert!(
        stand.log("signals.log").contains("term"),
        "SIGTERM first: signals {:?}, finished {:?}, calls {:?}",
        stand.log("signals.log"),
        stand.log("finished.log"),
        stand.calls()
    );
}

/// bugs+impl-1: an abort (the app quitting) ends a child in flight at
/// once, SIGTERM first.
#[test]
fn an_abort_ends_the_child_in_flight() {
    let stand = Stand::new();
    stand.answer(&stand.list("feat", 1), serde_json::json!([]));
    stand.stall(&stand.list("feat", 1), 30.0);
    let cancel = Arc::new(Cancel::default());
    let raised = cancel.clone();
    let calls = stand.dir.path().join("calls.log");
    std::thread::spawn(move || {
        // Once the child has logged its call, its trap is set.
        while !calls.exists() {
            std::thread::sleep(Duration::from_millis(10));
        }
        raised.abort();
    });
    let started = Instant::now();
    let error = stand
        .glab()
        .with_limits(Limits {
            abort_grace: Duration::from_secs(10),
            ..Limits::default()
        })
        .with_cancel(cancel)
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap_err();
    assert_eq!(error, Error::Stopped);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "not the 30 s sleep"
    );
    assert!(stand.log("signals.log").contains("term"));
}

/// bugs+impl-3: glab's config is YAML — quotes, inline comments, nulls
/// and flow values are read as YAML reads them.
#[test]
fn glab_config_values_are_read_as_yaml() {
    let config = r#"hosts:
    empty.quoted.example:
        token: "" # signed out
    empty.single.example:
        token: '' # signed out
    null.example:
        token: null # nothing
    tilde.example:
        token: ~
    flow.example:
        token: [] # not a string
    hash.inside.example:
        token: "abc#def" # a token with a hash
    plain.comment.example:
        token: abc # a comment
    keyring.quoted.example:
        use_keyring: "true" # in the keyring
    keyring.false.example:
        use_keyring: false # no
    "quoted.key.example":
        token: present
"#;
    let hosts = Hosts::parse(config);
    for host in [
        "empty.quoted.example",
        "empty.single.example",
        "null.example",
        "tilde.example",
        "flow.example",
        "keyring.false.example",
    ] {
        assert_eq!(hosts.login(host, false), Login::SignedOut, "{host}");
    }
    for host in [
        "hash.inside.example",
        "plain.comment.example",
        "keyring.quoted.example",
        "quoted.key.example",
    ] {
        assert_eq!(
            hosts.login(host, false),
            Login::SignedIn(host.into()),
            "{host}"
        );
    }
}

/// arch+quality-2: a host's own saved proxy is read, so Guild can tell
/// when glab would not use its network settings.
#[test]
fn a_host_s_saved_proxy_is_read() {
    let hosts = Hosts::parse(
        "hosts:\n    proxied.example:\n        token: present\n        proxy: \"http://glab-proxy:3128\" # its own\n    plain.example:\n        token: present\n",
    );
    assert_eq!(
        hosts.proxy("proxied.example").as_deref(),
        Some("http://glab-proxy:3128")
    );
    assert_eq!(hosts.proxy("plain.example"), None);
}

/// arch+quality-3: an https remote's port is part of its authority, as
/// glab keys its hosts; an ssh port is the transport's, not the API's.
#[test]
fn an_https_port_is_kept_and_an_ssh_port_is_not() {
    assert_eq!(
        Remote::parse("https://gitlab.example.com:8443/team/app.git").map(|r| r.host),
        Some("gitlab.example.com:8443".into())
    );
    assert_eq!(
        Remote::parse("ssh://git@gitlab.example.com:2222/team/app.git").map(|r| r.host),
        Some("gitlab.example.com".into())
    );
    let hosts = Hosts::parse("hosts:\n    gitlab.example.com:8443:\n        token: present\n");
    assert_eq!(
        hosts.login("gitlab.example.com:8443", false),
        Login::SignedIn("gitlab.example.com:8443".into())
    );
    assert_eq!(hosts.login("gitlab.example.com", false), Login::Unknown);
}

/// arch+quality-4: the config file glab 1.118 reads (config_file.go,
/// `SearchConfigFile`): `GLAB_CONFIG_DIR` alone when set, even without a
/// file; then the legacy `~/.config/glab-cli` when its file exists; then
/// XDG's config home — `XDG_CONFIG_HOME`, else macOS's Application Support.
#[test]
fn glab_s_config_file_is_the_one_glab_reads() {
    let home = tempfile::tempdir().unwrap();
    let put = |dir: &std::path::Path| {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("config.yml"), "hosts:\n").unwrap();
    };
    let legacy = home.path().join(".config/glab-cli");
    let mac = home.path().join("Library/Application Support/glab-cli");
    let xdg = home.path().join("xdg");
    put(&mac);
    assert_eq!(
        Hosts::config_file(home.path(), None, None),
        Some(mac.join("config.yml"))
    );
    put(&xdg.join("glab-cli"));
    assert_eq!(
        Hosts::config_file(home.path(), None, Some(xdg.as_os_str())),
        Some(xdg.join("glab-cli/config.yml")),
        "XDG_CONFIG_HOME is XDG's config home"
    );
    put(&legacy);
    assert_eq!(
        Hosts::config_file(home.path(), None, Some(xdg.as_os_str())),
        Some(legacy.join("config.yml")),
        "the legacy file wins over XDG"
    );
    let explicit = home.path().join("explicit-without-a-file");
    assert_eq!(
        Hosts::config_file(
            home.path(),
            Some(explicit.as_os_str()),
            Some(xdg.as_os_str())
        ),
        Some(explicit.join("config.yml")),
        "GLAB_CONFIG_DIR is authoritative even with no file"
    );
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(Hosts::config_file(empty.path(), None, None), None);
}

// --- code review r2

/// bugs+impl-8: a record that does not say its source branch cannot be
/// filtered out as another branch's; the batch is unknown, never "no MR".
#[test]
fn an_answer_without_its_source_branch_is_unknown_not_none() {
    for unsaid in [serde_json::Value::Null, "".into()] {
        let stand = Stand::new();
        let mut record = listed(1, "feat", "opened");
        match &unsaid {
            serde_json::Value::Null => {
                record.as_object_mut().unwrap().remove("source_branch");
            }
            empty => record["source_branch"] = empty.clone(),
        }
        stand.answer(&stand.list("feat", 1), serde_json::json!([record]));
        let read = stand
            .glab()
            .read(HOST, PROJECT, &branches(&["feat"]))
            .and_then(whole);
        assert!(
            matches!(read, Err(Error::Failed(_))),
            "{unsaid:?}: {read:?}"
        );
    }
}

/// bugs+impl-13: the detail is newer than the list; marked ready between
/// the two, the MR is not a draft.
#[test]
fn the_detail_s_draft_wins_over_the_list_s() {
    let stand = Stand::new();
    stand.open("feat", 1);
    let mut list = listed(1, "feat", "opened");
    list["draft"] = true.into();
    stand.answer(&stand.list("feat", 1), serde_json::json!([list]));
    let mut detail = listed(1, "feat", "opened");
    detail["draft"] = false.into();
    detail["detailed_merge_status"] = "mergeable".into();
    stand.answer(&Stand::base(1), detail);
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    assert!(!read[0].1.as_ref().unwrap().draft);

    // A detail that says nothing keeps the list's.
    let stand = Stand::new();
    stand.open("feat", 1);
    let mut list = listed(1, "feat", "opened");
    list["draft"] = true.into();
    stand.answer(&stand.list("feat", 1), serde_json::json!([list]));
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    assert!(read[0].1.as_ref().unwrap().draft);
}

/// adversarial-2: only an absolute http or https link is an MR's; anything
/// else is an incomplete answer, never a URL handed to the system.
#[test]
fn only_a_web_link_is_an_mr_s_link() {
    for url in [
        "",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "x-man-page://ls",
        "https://@/",
        "https://user@/team/app",
        "https://:443/",
        "http://exa mple.com/",
        "//gitlab.example.com/team/app/-/merge_requests/1",
        "https://",
    ] {
        let stand = Stand::new();
        stand.open("feat", 1);
        let mut record = listed(1, "feat", "merged");
        record["web_url"] = url.into();
        stand.answer(&stand.list("feat", 1), serde_json::json!([record]));
        let read = stand
            .glab()
            .read(HOST, PROJECT, &branches(&["feat"]))
            .and_then(whole);
        assert!(matches!(read, Err(Error::Failed(_))), "{url:?}: {read:?}");
    }
    for url in [
        "https://gitlab.example.com/team/app/-/merge_requests/1",
        "HTTP://gitlab.example.com:8080/team/app/-/merge_requests/1",
    ] {
        let stand = Stand::new();
        let mut record = listed(1, "feat", "merged");
        record["web_url"] = url.into();
        stand.answer(&stand.list("feat", 1), serde_json::json!([record]));
        let read = stand
            .glab()
            .read(HOST, PROJECT, &branches(&["feat"]))
            .and_then(whole)
            .unwrap();
        assert_eq!(read[0].1.as_ref().unwrap().web_url, url);
    }
}

/// bugs+impl-12: a deadline that passes while the child is prepared starts
/// no child.
#[test]
fn a_deadline_passed_in_preparing_starts_no_child() {
    use std::os::unix::process::CommandExt;
    let stand = Stand::new();
    stand.open("feat", 1);
    // A fork leaves a mark before anything of the child runs, so a child
    // ended at once by its deadline is still seen to have started.
    let mark = stand.dir.path().join("forked");
    let path = std::ffi::CString::new(mark.as_os_str().as_encoded_bytes()).unwrap();
    let deadline = Instant::now() + Duration::from_millis(300);
    let read = stand
        .glab()
        .with_prepare(Arc::new(move |command| {
            std::thread::sleep(Duration::from_millis(600));
            let path = path.clone();
            // SAFETY: open and close are async-signal-safe.
            unsafe {
                command.pre_exec(move || {
                    let fd = libc::open(path.as_ptr(), libc::O_CREAT | libc::O_WRONLY, 0o600);
                    if fd >= 0 {
                        libc::close(fd);
                    }
                    Ok(())
                });
            }
        }))
        .read_until(HOST, PROJECT, &branches(&["feat"]), deadline)
        .and_then(whole);
    assert_eq!(read, Err(Error::TimedOut));
    assert!(!mark.exists(), "a child was started after the deadline");
    assert!(stand.calls().is_empty(), "{:?}", stand.calls());
}

/// bugs+impl-9: GitLab's `not_approved` with no approvals to explain it —
/// the endpoint not offered — is said as GitLab's block, with no count.
#[test]
fn not_approved_without_approvals_is_gitlab_s_block() {
    use guild_session::merge_requests::status::{Block, Check, checklist};
    let stand = Stand::new();
    stand.open("feat", 1);
    let mut detail = listed(1, "feat", "opened");
    detail["detailed_merge_status"] = "not_approved".into();
    detail["head_pipeline"] = serde_json::json!({"status": "success"});
    stand.answer(&Stand::base(1), detail);
    stand.fail(
        &format!("{}/approvals", Stand::base(1)),
        "glab: 403 Forbidden (HTTP 403)",
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    let mr = read[0].1.as_ref().unwrap();
    assert_eq!(mr.approvals, None);
    let lines = checklist(mr, None);
    assert!(
        lines
            .iter()
            .any(|line| line.check == Check::Blocked(Block::NotApproved) && line.ok == Some(false)),
        "{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| matches!(line.check, Check::Approvals { .. })),
        "no count: {lines:?}"
    );
}

/// arch+quality-2: a relative `GLAB_CONFIG_DIR` is made absolute before the
/// child, which runs in `/`, sees it — the same directory the local read
/// resolved.
#[test]
fn a_relative_config_dir_reaches_glab_absolute() {
    use guild_session::merge_requests::glab::absolute_config_dir;
    let stand = Stand::new();
    stand.open("feat", 1);
    let mut env = Stand::base_env();
    env.push(("ENV_SEEN".into(), "GLAB_CONFIG_DIR".into()));
    env.push(("GLAB_CONFIG_DIR".into(), "relative/glab".into()));
    stand
        .glab()
        .with_environment(env)
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    let expected = std::env::current_dir().unwrap().join("relative/glab");
    assert_eq!(
        absolute_config_dir(std::ffi::OsStr::new("relative/glab")),
        expected
    );
    let seen = stand.env_seen();
    assert!(!seen.is_empty());
    for line in seen {
        assert_eq!(line, format!("GLAB_CONFIG_DIR={}", expected.display()));
    }
}

/// bugs+impl-3: an empty block scalar is empty — `|-`, `|` or `>` with
/// nothing more indented after it — and a block with a line is a value.
#[test]
fn an_empty_block_scalar_is_no_token() {
    let hosts = Hosts::parse(
        "hosts:
    strip.example:
        token: |-
        user: someone
    literal.example:
        token: |
    folded.example:
        token: >2
        user: someone
    blank.example:
        token: |-

        user: someone
    filled.example:
        token: |-
            glpat-present
        user: someone
    keyring.block.example:
        use_keyring: |-
            true
",
    );
    for host in [
        "strip.example",
        "literal.example",
        "folded.example",
        "blank.example",
    ] {
        assert_eq!(hosts.login(host, false), Login::SignedOut, "{host}");
    }
    for host in ["filled.example", "keyring.block.example"] {
        assert_eq!(
            hosts.login(host, false),
            Login::SignedIn(host.into()),
            "{host}"
        );
    }
}

/// arch+quality-3: an `ssh_host` names a glab host for an ssh or scp-style
/// remote only; an https remote matches a glab host by its own name.
#[test]
fn an_ssh_alias_is_only_for_ssh_remotes() {
    let hosts = Hosts::parse(
        "hosts:\n    one.example:\n        token: present\n        ssh_host: two.example\n",
    );
    let https = Remote::parse("https://two.example/team/app.git").unwrap();
    let scp = Remote::parse("git@two.example:team/app.git").unwrap();
    let ssh = Remote::parse("ssh://git@two.example/team/app.git").unwrap();
    assert!(!https.ssh && scp.ssh && ssh.ssh);
    assert_eq!(hosts.login(&https.host, https.ssh), Login::Unknown);
    assert_eq!(
        hosts.login(&scp.host, scp.ssh),
        Login::SignedIn("one.example".into())
    );
    assert_eq!(
        hosts.login(&ssh.host, ssh.ssh),
        Login::SignedIn("one.example".into())
    );
}

/// adversarial-4: a host's `subfolder` is in glab's API base already; the
/// project is the path after it, cut at a segment boundary.
#[test]
fn a_host_s_subfolder_is_not_part_of_the_project() {
    let hosts = Hosts::parse(
        "hosts:\n    git.example:\n        token: present\n        subfolder: /gitlab/\n    plain.example:\n        token: present\n",
    );
    assert_eq!(hosts.project("git.example", "gitlab/team/app"), "team/app");
    assert_eq!(
        hosts.project("git.example", "gitlabx/team/app"),
        "gitlabx/team/app"
    );
    assert_eq!(hosts.project("git.example", "team/app"), "team/app");
    assert_eq!(
        hosts.project("plain.example", "gitlab/team/app"),
        "gitlab/team/app"
    );
}

/// bugs+impl-1: the window gets 200 ms to quit, and a glab deaf to SIGTERM
/// would outlive it; the quit's last moment kills every live child.
///
/// The kill reaches every glab child of the process, so the test runs in a
/// process of its own, where no other test's child is alive.
#[test]
fn the_quit_s_last_moment_kills_what_sigterm_did_not() {
    const NAME: &str = "merge_requests_glab::the_quit_s_last_moment_kills_what_sigterm_did_not";
    if std::env::var_os("GLD063_QUIT_ALONE").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([NAME, "--exact", "--test-threads=1", "--nocapture"])
            .env("GLD063_QUIT_ALONE", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let stand = Stand::new();
    stand.answer(&stand.list("feat", 1), serde_json::json!([]));
    stand.stall(&stand.list("feat", 1), 30.0);
    stand.deaf(&stand.list("feat", 1));
    let cancel = Arc::new(Cancel::default());
    let raised = cancel.clone();
    let calls = stand.dir.path().join("calls.log");
    std::thread::spawn(move || {
        while !calls.exists() {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(200));
        raised.abort();
        std::thread::sleep(Duration::from_millis(150));
        guild_session::merge_requests::glab::kill_every_child();
    });
    let started = Instant::now();
    let error = stand
        .glab()
        .with_limits(Limits {
            abort_grace: Duration::from_secs(20),
            ..Limits::default()
        })
        .with_cancel(cancel)
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap_err();
    assert_eq!(error, Error::Stopped);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "killed, not waited out: {:?}",
        started.elapsed()
    );
    assert_eq!(stand.log("finished.log"), "", "the sleep did not run out");
}

// --- code review r3

/// lenses-4: whether a host is signed out is GitLab's 401, read from the
/// status line `--include` prints — never from a message's words. glab
/// 1.118's real outputs, measured against a local server: a project's 403
/// whose message says "not authenticated", and a 401 whose `errors` array
/// glab prints with no status at all.
#[test]
fn signed_out_is_gitlab_s_401_not_a_message_s_words() {
    let stand = Stand::new();
    stand.raw(
        &stand.list("feat", 1),
        "403 Forbidden",
        r#"{"message": "not authenticated for this project"}"#,
        Some("glab: not authenticated for this project (HTTP 403)"),
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole);
    assert!(matches!(read, Err(Error::Failed(_))), "{read:?}");
    assert!(
        stand.calls()[0].contains("--include"),
        "{:?}",
        stand.calls()
    );

    let stand = Stand::new();
    stand.raw(
        &stand.list("feat", 1),
        "401 Unauthorized",
        r#"{"errors": ["Unauthorized"]}"#,
        Some("glab: Unauthorized"),
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole);
    assert_eq!(read, Err(Error::SignedOut { host: HOST.into() }));

    // A whole answer behind its headers reads as before, and no header
    // reaches a failure's words.
    let stand = Stand::new();
    stand.open("feat", 1);
    let mut list = listed(1, "feat", "opened");
    list["title"] = "With headers".into();
    stand.raw(
        &stand.list("feat", 1),
        "200 OK",
        &serde_json::json!([list]).to_string(),
        None,
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
        .unwrap();
    assert_eq!(read[0].1.as_ref().unwrap().iid, 1);
    let stand = Stand::new();
    stand.raw(
        &stand.list("feat", 1),
        "404 Not Found",
        r#"{"message": "404 Project Not Found"}"#,
        Some("glab: 404 Project Not Found (HTTP 404)"),
    );
    match stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat"]))
        .and_then(whole)
    {
        Err(Error::Failed(line)) => assert!(!line.contains("X-Request-Id"), "{line}"),
        other => panic!("{other:?}"),
    }
}

/// lenses-8: a node's tag or anchor comes before its block header; a
/// tagged empty block is still empty.
#[test]
fn a_tagged_empty_block_scalar_is_no_token() {
    let hosts = Hosts::parse(
        "hosts:
    tagged.example:
        token: !!str |-
        user: someone
    anchored.example:
        token: &t |
        user: someone
    tagged.filled.example:
        token: !!str |-
            glpat-present
",
    );
    assert_eq!(hosts.login("tagged.example", false), Login::SignedOut);
    assert_eq!(hosts.login("anchored.example", false), Login::SignedOut);
    assert_eq!(
        hosts.login("tagged.filled.example", false),
        Login::SignedIn("tagged.filled.example".into())
    );
}

// --- the author and the account (mr-header-status-and-author)

/// The list's `author.id` is the MR's author; a list without one says
/// nothing of its author.
#[test]
fn an_mr_s_author_is_read_from_its_list() {
    let stand = Stand::new();
    stand.open("feat", 294);
    let mut with_author = listed(294, "feat", "opened");
    with_author["author"] = serde_json::json!({"id": 42, "username": "me"});
    stand.answer(&stand.list("feat", 1), serde_json::json!([with_author]));
    stand.answer(
        &stand.list("other", 1),
        serde_json::json!([listed(7, "other", "merged")]),
    );
    let read = stand
        .glab()
        .read(HOST, PROJECT, &branches(&["feat", "other"]))
        .and_then(whole)
        .unwrap();
    assert_eq!(read[0].1.as_ref().unwrap().author, Some(42));
    assert_eq!(read[1].1.as_ref().unwrap().author, None);
}

/// The account glab is logged in with on a host: `user`'s `id`, asked with
/// the host pinned, as every other read.
#[test]
fn the_account_is_glab_s_user_on_the_host() {
    let stand = Stand::new();
    stand.answer("user", serde_json::json!({"id": 42, "username": "me"}));
    let account = stand
        .glab()
        .account(HOST, Instant::now() + Duration::from_secs(10));
    assert_eq!(account, Ok(42));
    assert_eq!(
        stand.calls(),
        vec![format!("api --include --hostname {HOST} user")]
    );
}

/// An answer without an id, a failure or a stall is an error, never an
/// account.
#[test]
fn an_account_glab_cannot_say_is_an_error() {
    let stand = Stand::new();
    stand.answer("user", serde_json::json!({"username": "me"}));
    assert!(
        stand
            .glab()
            .account(HOST, Instant::now() + Duration::from_secs(10))
            .is_err()
    );
    let stand = Stand::new();
    stand.fail("user", "glab: 500 Internal Server Error (HTTP 500)");
    assert!(
        stand
            .glab()
            .account(HOST, Instant::now() + Duration::from_secs(10))
            .is_err()
    );
    let stand = Stand::new();
    stand.answer("user", serde_json::json!({"id": 42}));
    stand.stall("user", 5.0);
    assert_eq!(
        stand
            .glab()
            .account(HOST, Instant::now() + Duration::from_millis(300)),
        Err(Error::TimedOut)
    );
}
