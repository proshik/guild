//! Noninteractive authorized transport: gh owns credentials and redirects.
use super::{Error, Request, check_cancel};
use guild_session::process::ProcessGroup;
use std::{
    ffi::OsStr,
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{atomic::AtomicBool, mpsc},
    time::{Duration, Instant},
};

fn executable() -> Result<PathBuf, Error> {
    let paths = guild_session::login_path::path_or_inherited();
    for directory in std::env::split_paths(OsStr::new(&paths)) {
        if !directory.is_absolute() {
            continue;
        }
        let path = directory.join("gh");
        if path
            .metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        {
            return path.canonicalize().map_err(|_| Error::MissingGh);
        }
    }
    Err(Error::MissingGh)
}
/// Stream one explicitly addressed API response without extracting a token or running a shell.
pub(super) fn request(
    request: &Request<'_>,
    cancel: &AtomicBool,
    output: &mut dyn Write,
    progress: &mut dyn FnMut(u64),
    override_path: Option<&Path>,
    network: &guild_session::network::Network,
) -> Result<u64, Error> {
    let Request {
        path,
        limit,
        asset,
        timeout,
    } = *request;
    check_cancel(cancel)?;
    let executable = override_path
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(executable)?;
    let mut command = Command::new(executable);
    command
        .args([
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            "--include",
            "--header",
            if asset {
                "Accept: application/octet-stream"
            } else {
                "Accept: application/vnd.github+json"
            },
            "--header",
            "X-GitHub-Api-Version: 2026-03-10",
            path.trim_start_matches('/'),
        ])
        .env("GH_PROMPT_DISABLED", "1")
        .env_remove("GH_DEBUG")
        .env_remove("GH_API_HOST")
        .env_remove("GH_HTTP_UNIX_SOCKET")
        .env("GH_HOST", "github.com")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let proxy = network
        .policy(guild_session::network::Scope::Guild)
        .map_err(|_| Error::Invalid)?;
    crate::network::apply_policy(&mut command, proxy.as_ref());
    let mut child = ProcessGroup::spawn(&mut command).map_err(|_| Error::MissingGh)?;
    let mut stdout = child.take_stdout().ok_or(Error::Network)?;
    let stderr = child.take_stderr().ok_or(Error::Network)?;
    let (tx, rx) = mpsc::sync_channel(4);
    let reader = std::thread::spawn(move || {
        let mut buffer = [0u8; 65536];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => {
                    let _ = tx.send(Ok(Vec::new()));
                    break;
                }
                Ok(n) => {
                    if tx.send(Ok(buffer[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = tx.send(Err(()));
                    break;
                }
            }
        }
    });
    // Drain bounded diagnostics privately. No raw stderr ever enters an error or log.
    let diagnostics = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.take(65537).read_to_end(&mut bytes).is_ok() && bytes.len() <= 65536
    });
    let deadline = Instant::now() + timeout;
    let mut last_input = Instant::now();
    let mut header = Vec::new();
    let mut headers_done = false;
    let mut total = 0u64;
    let result = (|| {
        loop {
            check_cancel(cancel)?;
            if Instant::now() >= deadline
                || (headers_done && last_input.elapsed() > Duration::from_secs(1))
            {
                return Err(Error::Network);
            }
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(Ok(data)) if data.is_empty() => break,
                Ok(Ok(data)) => {
                    last_input = Instant::now();
                    let mut payload = data.as_slice();
                    if !headers_done {
                        header.extend_from_slice(&data);
                        let boundary = header
                            .windows(4)
                            .position(|p| p == b"\r\n\r\n")
                            .map(|p| (p, 4))
                            .or_else(|| {
                                header.windows(2).position(|p| p == b"\n\n").map(|p| (p, 2))
                            });
                        let Some((at, separator)) = boundary else {
                            if header.len() > 65536 {
                                return Err(Error::Limit);
                            }
                            continue;
                        };
                        if at > 65536 {
                            return Err(Error::Limit);
                        }
                        classify(&header[..at])?;
                        headers_done = true;
                        payload = &header[at + separator..];
                    }
                    total = total
                        .checked_add(payload.len() as u64)
                        .ok_or(Error::Limit)?;
                    if total > limit {
                        return Err(Error::Limit);
                    }
                    output.write_all(payload).map_err(|_| Error::Storage)?;
                    progress(total);
                    if !header.is_empty() {
                        header.clear();
                    }
                }
                Ok(Err(())) => return Err(Error::Network),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(Error::Network),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        if !headers_done {
            return Err(Error::Access);
        }
        loop {
            check_cancel(cancel)?;
            if Instant::now() >= deadline {
                return Err(Error::Network);
            }
            match child.try_wait().map_err(|_| Error::Network)? {
                Some(status) => {
                    return if status.success() {
                        Ok(total)
                    } else {
                        Err(Error::Access)
                    };
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    })();
    // Dropping the receiver first unblocks a producer waiting on the bounded queue.
    drop(rx);
    let stopped = child.stop();
    let read = reader.join();
    let diagnostics = diagnostics.join();
    if stopped.is_err() || read.is_err() || !diagnostics.unwrap_or(false) {
        return Err(Error::Network);
    }
    result
}
fn classify(bytes: &[u8]) -> Result<(), Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    let mut lines = text.lines();
    let first = lines.next().ok_or(Error::Invalid)?;
    if !first.starts_with("HTTP/") {
        return Err(Error::Invalid);
    }
    let status = first
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or(Error::Invalid)?;
    let mut remaining = None;
    let mut retry = None;
    let mut reset = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            match name.to_ascii_lowercase().as_str() {
                "x-ratelimit-remaining" => remaining = Some(value.trim()),
                "retry-after" => retry = Some(value.trim()),
                "x-ratelimit-reset" => reset = Some(value.trim()),
                _ => {}
            }
        }
    }
    if let Some(error) = super::rate_limit(status, remaining, reset, retry) {
        return Err(error);
    }
    if [401, 403, 404].contains(&status) {
        return Err(Error::Access);
    }
    if status != 200 {
        return Err(Error::Network);
    }
    Ok(())
}
