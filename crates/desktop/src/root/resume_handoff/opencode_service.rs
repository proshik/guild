//! Read-only shared-service evidence for managed standalone continuation only.
use serde_json::Value;
use std::path::PathBuf;

type Check<T> = Result<T, String>;
#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    dev: u64,
    ino: u64,
    len: u64,
    seconds: i64,
    nanos: i64,
}
// Never Debug/Serialize/Clone the credential-bearing registration.
struct Registration {
    path: PathBuf,
    stamp: FileStamp,
    id: String,
    version: String,
    url: String,
    pid: u32,
    password: String,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Witness {
    registration: FileStamp,
    instance: String,
    endpoint: String,
    pid: u32,
    birth: u64,
    database: (u64, u64),
}
fn gap(message: &str) -> String {
    format!("opencode_inspection: {message}; no input or launch")
}
fn holders(ps: &str) -> Check<Vec<(u32, &str)>> {
    super::checked_holders(ps, "inspection-validation")?;
    Ok(ps
        .lines()
        .filter_map(|line| {
            let mut words = line.trim_start().splitn(2, char::is_whitespace);
            let pid = words.next()?.parse().ok()?;
            let (_, args) = words.next()?.trim_start().split_once(char::is_whitespace)?;
            guild_session::HarnessKind::OpenCode
                .impl_()
                .detect_in_front(args.trim_start())
                .then_some((pid, args.trim_start()))
        })
        .collect())
}
fn only_service(ps: &str, reg: &Registration) -> Check<()> {
    let rows = holders(ps)?;
    if rows.len() != 1 || rows[0].0 != reg.pid {
        return Err(gap(
            "unbound OpenCode process; existing service was not excluded",
        ));
    }
    let words: Vec<_> = rows[0].1.split_whitespace().collect();
    if words.len() != 3 || words[1..] != ["serve", "--service"] {
        return Err(gap(
            "registered process is not the supported service command",
        ));
    }
    Ok(())
}
fn inspect_with(
    allow_service: bool,
    conversation: &str,
    mut processes: impl FnMut() -> Check<String>,
    registration: impl FnOnce() -> Check<Registration>,
    mut local: impl FnMut(&Registration) -> Check<Witness>,
    mut history: impl FnMut() -> Check<()>,
    mut get: impl FnMut(&Registration, &str) -> Check<Value>,
) -> Check<Option<Witness>> {
    if conversation.len() > 256
        || !conversation.starts_with("ses_")
        || !conversation
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(gap("invalid exact conversation"));
    }
    let ps = processes()?;
    if holders(&ps)?.is_empty() {
        history()?;
        if !holders(&processes()?)?.is_empty() {
            return Err(gap("process snapshot changed"));
        }
        return Ok(None);
    }
    let candidates = holders(&ps)?;
    if candidates.len() != 1 || {
        let words: Vec<_> = candidates[0].1.split_whitespace().collect();
        words.len() != 3 || words[1..] != ["serve", "--service"]
    } {
        return Err(gap(
            "OpenCode runtime holder exists; exact ownership is unknown",
        ));
    }
    if !allow_service {
        return Err(gap(
            "custom launch prefix/arguments cannot qualify service exclusion",
        ));
    }
    let reg = registration()?;
    only_service(&ps, &reg)?; // Before history subprocesses or shared-service HTTP.
    let before = local(&reg)?;
    history()?;
    let info = get(&reg, "/api/info")?;
    if info["pid"].as_u64() != Some(reg.pid.into())
        || info["version"].as_str() != Some(&reg.version)
        || !info["urls"].as_array().is_some_and(|urls| {
            urls.iter().any(|v| {
                v.as_str()
                    .is_some_and(|s| s.trim_end_matches('/') == reg.url)
            })
        })
    {
        return Err(gap("API/registration identity differs"));
    }
    let active = get(&reg, "/api/session/active")?;
    let active = active["data"]
        .as_object()
        .ok_or_else(|| gap("active response is unsupported"))?;
    if active.contains_key(conversation) {
        return Err(gap("target execution is active or waiting"));
    }
    for tail in ["inbox", "form"] {
        let response = get(&reg, &format!("/api/session/{conversation}/{tail}"))?;
        let rows = response["data"]
            .as_array()
            .ok_or_else(|| gap("pending-input response is unsupported"))?;
        if !rows.is_empty() {
            return Err(gap("target has pending input or a human form"));
        }
    }
    only_service(&processes()?, &reg)?;
    let after = local(&reg)?;
    if before != after {
        return Err(gap("service/storage generation changed"));
    }
    Ok(Some(after))
}

pub(super) fn inspect(
    record: &guild_session::SessionRecord,
    conversation: &str,
    allow: bool,
) -> Check<Option<Witness>> {
    let path = guild_session::login_path::path()
        .or_else(|| std::env::var_os("PATH"))
        .ok_or_else(|| gap("executable PATH unavailable"))?;
    inspect_with(
        allow,
        conversation,
        super::super::workflow::continuation_process_snapshot,
        || read_registration(&registration_path()?),
        platform,
        || guild_session::history::opencode::qualify_conversation(&path, &record.cwd, conversation),
        get,
    )
}

fn registration_path() -> Check<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .ok_or_else(|| gap("state directory unavailable"))?;
    Ok(state.join("opencode/service.json"))
}
fn stamp(meta: &std::fs::Metadata) -> FileStamp {
    use std::os::unix::fs::MetadataExt;
    FileStamp {
        dev: meta.dev(),
        ino: meta.ino(),
        len: meta.len(),
        seconds: meta.mtime(),
        nanos: meta.mtime_nsec(),
    }
}
fn registration_file(path: &std::path::Path) -> Check<std::fs::File> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::Component,
    };
    if !path.is_absolute() {
        return Err(gap("registration path is not absolute"));
    }
    let fd = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(gap("registration directory unavailable"));
    }
    let mut directory = unsafe { std::fs::File::from_raw_fd(fd) };
    let components: Vec<_> = path.components().skip(1).collect();
    for (at, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(gap("registration path is unsupported"));
        };
        let name =
            CString::new(name.as_bytes()).map_err(|_| gap("registration path is invalid"))?;
        let last = at + 1 == components.len();
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | if last { 0 } else { libc::O_DIRECTORY };
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(gap("registration unavailable or symlinked"));
        }
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        if last {
            let meta = file
                .metadata()
                .map_err(|_| gap("registration metadata unavailable"))?;
            if !meta.is_file()
                || meta.uid() != unsafe { libc::geteuid() }
                || meta.mode() & 0o077 != 0
                || meta.len() == 0
                || meta.len() > 65536
            {
                return Err(gap(
                    "registration must be a private same-user bounded regular file",
                ));
            }
            return Ok(file);
        }
        directory = file;
    }
    Err(gap("registration path is empty"))
}
fn read_registration(path: &std::path::Path) -> Check<Registration> {
    use std::io::Read;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Raw {
        id: String,
        version: String,
        url: String,
        pid: u32,
        password: String,
    }
    let file = registration_file(path)?;
    let before = file
        .metadata()
        .map_err(|_| gap("registration metadata unavailable"))?;
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| gap("registration read failed"))?;
    let raw: Raw =
        serde_json::from_slice(&bytes).map_err(|_| gap("registration schema unsupported"))?;
    if bytes.len() as u64 != before.len()
        || raw.pid == 0
        || raw.id.is_empty()
        || raw.id.len() > 128
        || raw.version != "2.0.21"
        || raw.version.len() > 64
        || raw.password.is_empty()
        || raw.password.len() > 4096
        || !raw.password.bytes().all(|b| (32..127).contains(&b))
    {
        return Err(gap("registration identity/auth unsupported"));
    }
    let url = raw.url.trim_end_matches('/').to_owned();
    let parsed = gpui::http_client::Url::parse(&url)
        .map_err(|_| gap("registration endpoint unsupported"))?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| gap("registration port missing"))?;
    if port == 0 || url != format!("http://127.0.0.1:{port}") {
        return Err(gap("only literal loopback HTTP is supported"));
    }
    let current = registration_file(path)?
        .metadata()
        .map_err(|_| gap("registration metadata unavailable"))?;
    if stamp(&before) != stamp(&current) {
        return Err(gap("registration changed during read"));
    }
    Ok(Registration {
        path: path.into(),
        stamp: stamp(&before),
        id: raw.id,
        version: raw.version,
        url,
        pid: raw.pid,
        password: raw.password,
    })
}
fn command(program: &str, args: &[&str]) -> Check<std::process::Output> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    let out = super::inspect_command(
        cmd,
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    )?;
    if !out.stderr.is_empty() {
        return Err(gap("local inspection warning"));
    }
    Ok(out)
}
fn platform(reg: &Registration) -> Check<Witness> {
    use std::os::unix::fs::MetadataExt;
    let file = registration_file(&reg.path)?
        .metadata()
        .map_err(|_| gap("registration metadata unavailable"))?;
    if stamp(&file) != reg.stamp {
        return Err(gap("registration changed"));
    }
    let birth = crate::terminal::ownership::monotonic_birth(reg.pid as i32)
        .ok_or_else(|| gap("service generation unavailable"))?;
    let mut process: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = unsafe {
        libc::proc_pidinfo(
            reg.pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut process as *mut libc::proc_bsdinfo).cast(),
            std::mem::size_of_val(&process) as i32,
        )
    };
    if size as usize != std::mem::size_of_val(&process)
        || process.pbi_uid != unsafe { libc::geteuid() }
        || process.pbi_status == libc::SZOMB
    {
        return Err(gap("service process identity unavailable"));
    }
    let pid = reg.pid.to_string();
    let port = reg
        .url
        .rsplit_once(':')
        .ok_or_else(|| gap("endpoint unavailable"))?
        .1;
    let selector = format!("-iTCP:{port}");
    let listener = command(
        "/usr/sbin/lsof",
        &["-nP", "-a", "-p", &pid, &selector, "-sTCP:LISTEN", "-Fpn"],
    )?;
    let text = std::str::from_utf8(&listener.stdout).map_err(|_| gap("listener output invalid"))?;
    let pids: Vec<_> = text.lines().filter_map(|v| v.strip_prefix('p')).collect();
    if !listener.status.success()
        || pids != [pid.as_str()]
        || !text.lines().any(|v| v == format!("n127.0.0.1:{port}"))
    {
        return Err(gap("listener/registration identity differs"));
    }
    let clients = command(
        "/usr/sbin/lsof",
        &["-nP", &selector, "-sTCP:ESTABLISHED", "-Fpn"],
    )?;
    if clients.status.code() != Some(1) || !clients.stdout.is_empty() {
        return Err(gap("other established client or uncertain client snapshot"));
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok_or_else(|| gap("history storage unavailable"))?
        .join("opencode");
    let selected = std::env::var_os("OPENCODE_DB").unwrap_or_else(|| "opencode.db".into());
    if selected == ":memory:" {
        return Err(gap("in-memory history cannot qualify service exclusion"));
    }
    let selected = PathBuf::from(selected);
    let db = if selected.is_absolute() {
        selected
    } else {
        data.join(selected)
    };
    let db = db
        .canonicalize()
        .map_err(|_| gap("history database unavailable"))?;
    let meta = db
        .metadata()
        .map_err(|_| gap("history database unavailable"))?;
    if !meta.is_file() {
        return Err(gap("history database is not regular"));
    }
    let files = command("/usr/sbin/lsof", &["-nP", "-a", "-p", &pid, "-Fn"])?;
    let text = std::str::from_utf8(&files.stdout).map_err(|_| gap("storage output invalid"))?;
    if !files.status.success()
        || !text.lines().any(|v| {
            v.strip_prefix('n')
                .is_some_and(|s| std::path::Path::new(s) == db)
        })
    {
        return Err(gap("service storage binding differs or is unknown"));
    }
    if crate::terminal::ownership::monotonic_birth(reg.pid as i32) != Some(birth) {
        return Err(gap("service generation changed"));
    }
    Ok(Witness {
        registration: reg.stamp.clone(),
        instance: reg.id.clone(),
        endpoint: reg.url.clone(),
        pid: reg.pid,
        birth,
        database: (meta.dev(), meta.ino()),
    })
}
fn get(reg: &Registration, path: &str) -> Check<Value> {
    let escaped = reg.password.replace('\\', "\\\\").replace('"', "\\\"");
    let config = format!("user = \"opencode:{escaped}\"\n");
    let mut cmd = std::process::Command::new("/usr/bin/curl");
    cmd.args([
        "-q",
        "--silent",
        "--show-error",
        "--proxy",
        "",
        "--noproxy",
        "*",
        "--proto",
        "=http",
        "--max-redirs",
        "0",
        "--retry",
        "0",
        "--connect-timeout",
        "1",
        "--max-time",
        "2",
        "--max-filesize",
        "1048576",
        "--request",
        "GET",
        "--config",
        "-",
        "--write-out",
        "\n%{http_code}",
        "--url",
    ]);
    cmd.arg(format!("{}{path}", reg.url));
    // Fixed safe operation metadata only: no credential, body, path ID or raw error.
    tracing::info!("managed OpenCode service inspection GET");
    let out = super::inspect_command_input(
        cmd,
        std::time::Instant::now() + std::time::Duration::from_secs(3),
        Some(config.into_bytes()),
    )?;
    if !out.status.success() || !out.stderr.is_empty() {
        return Err(gap("HTTP transport failed"));
    }
    let split = out
        .stdout
        .iter()
        .rposition(|b| *b == b'\n')
        .ok_or_else(|| gap("HTTP status missing"))?;
    if &out.stdout[split + 1..] != b"200" {
        return Err(gap("HTTP status refused"));
    }
    serde_json::from_slice(&out.stdout[..split]).map_err(|_| gap("HTTP response unsupported"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    fn registration() -> Registration {
        Registration {
            path: PathBuf::from("/fixture/service.json"),
            stamp: FileStamp {
                dev: 1,
                ino: 2,
                len: 3,
                seconds: 4,
                nanos: 5,
            },
            id: "instance".into(),
            version: "2.0.21".into(),
            url: "http://127.0.0.1:1234".into(),
            pid: 42,
            password: "fixture-only-not-a-user-secret".into(),
        }
    }
    fn witness(reg: &Registration) -> Witness {
        Witness {
            registration: reg.stamp.clone(),
            instance: reg.id.clone(),
            endpoint: reg.url.clone(),
            pid: reg.pid,
            birth: 100,
            database: (8, 9),
        }
    }
    fn execute(case: &str) -> (Check<Option<Witness>>, Vec<String>) {
        let requests = RefCell::new(Vec::new());
        let scans = Cell::new(0);
        let locals = Cell::new(0);
        let result = inspect_with(
            case != "prefix",
            "ses_exact",
            || {
                let scan = scans.get();
                scans.set(scan + 1);
                Ok(
                    if case == "foreign" || (case == "late-foreign" && scan > 0) {
                        "42 1 opencode serve --service\n43 1 opencode".into()
                    } else {
                        "42 1 opencode serve --service".into()
                    },
                )
            },
            || {
                if case == "missing-registration" {
                    Err(gap("registration unavailable"))
                } else {
                    Ok(registration())
                }
            },
            |reg| {
                let n = locals.get();
                locals.set(n + 1);
                if case == "client" {
                    return Err(gap("other established client"));
                }
                let mut w = witness(reg);
                if case == "reused" && n > 0 {
                    w.birth += 1;
                }
                Ok(w)
            },
            || {
                if case == "missing-history" {
                    Err(gap("exact history missing"))
                } else {
                    Ok(())
                }
            },
            |reg, path| {
                requests.borrow_mut().push(path.to_owned());
                if case == "auth" {
                    return Err(gap("HTTP authentication failed"));
                }
                Ok(if path == "/api/info" {
                    serde_json::json!({"pid":if case=="api-pid" {43} else {reg.pid},"version":reg.version,"urls":[reg.url],"paths":{"tmp":"/fixture"}})
                } else if path == "/api/session/active" {
                    if case == "malformed" {
                        serde_json::json!({"data":[]})
                    } else if case == "busy" {
                        serde_json::json!({"data":{"ses_exact":{}}})
                    } else {
                        serde_json::json!({"data":{"ses_other":{}}})
                    }
                } else if case == "inbox" && path.ends_with("/inbox")
                    || case == "forms" && path.ends_with("/form")
                {
                    serde_json::json!({"data":[{}]})
                } else {
                    serde_json::json!({"data":[]})
                })
            },
        );
        (result, requests.into_inner())
    }
    #[test]
    fn private_registration_rejects_symlinks_permissions_remote_and_schema_changes() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let file = root.join("service.json");
        let valid = serde_json::json!({"id":"fixture","version":"2.0.21","url":"http://127.0.0.1:1234","pid":42,"password":"fixture-only"});
        let write = |value: &Value| {
            std::fs::write(&file, value.to_string()).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        };
        write(&valid);
        assert!(read_registration(&file).is_ok());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_registration(&file).is_err());
        write(&valid);
        let link = root.join("link");
        symlink(&file, &link).unwrap();
        assert!(read_registration(&link).is_err());
        let parent = root.join("parent");
        symlink(&root, &parent).unwrap();
        assert!(read_registration(&parent.join("service.json")).is_err());
        for key in ["url", "unexpected", "version"] {
            let mut invalid = valid.clone();
            invalid[key] = "http://localhost:1234".into();
            write(&invalid);
            assert!(read_registration(&file).is_err());
        }
    }
    #[test]
    fn loopback_curl_sends_fixture_auth_only_over_stdin_and_refuses_http_errors() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        for status in [200, 401, 302] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut reg = registration();
            reg.url = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    request.push(byte[0]);
                }
                let text = String::from_utf8(request).unwrap();
                assert!(text.starts_with("GET /api/info HTTP/1.1\r\n"));
                assert!(text.lines().any(|line| {
                    line.to_ascii_lowercase()
                        .starts_with("authorization: basic ")
                }));
                let body = r#"{"fixture":true}"#;
                write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            });
            let result = get(&reg, "/api/info");
            server.join().unwrap();
            assert_eq!(result.is_ok(), status == 200);
        }
    }
    #[test]
    fn idle_registered_service_uses_four_gets_without_changing_standalone_delivery() {
        let (result, requests) = execute("idle");
        assert!(result.is_ok(), "{}", result.err().unwrap_or_default());
        assert!(result.unwrap().is_some());
        assert_eq!(
            requests,
            [
                "/api/info",
                "/api/session/active",
                "/api/session/ses_exact/inbox",
                "/api/session/ses_exact/form"
            ]
        );
    }
    #[test]
    fn unsafe_snapshots_auth_and_pending_human_inputs_never_qualify_service() {
        for case in [
            "foreign",
            "late-foreign",
            "prefix",
            "missing-registration",
            "client",
            "reused",
            "missing-history",
            "auth",
            "api-pid",
            "busy",
            "malformed",
            "inbox",
            "forms",
        ] {
            let (result, requests) = execute(case);
            assert!(result.is_err(), "unsafe service admitted: {case}");
            assert!(requests.len() <= 4);
            if [
                "foreign",
                "prefix",
                "missing-registration",
                "client",
                "missing-history",
            ]
            .contains(&case)
            {
                assert!(
                    requests.is_empty(),
                    "unqualified admission used HTTP: {case}"
                );
            }
        }
    }
}
