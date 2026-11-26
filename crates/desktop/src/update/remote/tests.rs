use super::*;
use guild_session::update::{self, BuildInfo, CiIdentity, Provenance, SigningKey};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};
fn fixture(tag: &str, channel: Channel) -> (Manifest, Vec<u8>, Vec<u8>, KeyRing) {
    let key = SigningKey::from_bytes(&[7; 32]);
    let keys = BTreeMap::from([("fixture".into(), key.verifying_key().to_bytes())]);
    let mut build = BuildInfo::local("1".repeat(40));
    build.tag = Some(tag.into());
    build.channel = Some(channel);
    build.revision_order = Some(50);
    build.provenance = Provenance::GithubActions;
    build.ci = Some(CiIdentity {
        repository: "owner/guild".into(),
        run_id: 123,
        attempt: 1,
    });
    let manifest = Manifest {
        schema: 1,
        app_id: update::APP_ID.into(),
        repository: "owner/guild".into(),
        build,
        archive: "Guild.zip".into(),
        bytes: 10,
        sha256: "0".repeat(64),
        notes: "Fixture notes".into(),
    };
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let signature = update::sign_manifest(&bytes, &key, "fixture").unwrap();
    (manifest, bytes, signature, keys)
}
fn release(tag: &str, prerelease: bool) -> serde_json::Value {
    serde_json::json!({"id":1,"tag_name":tag,"prerelease":prerelease,"draft":false,"assets":[{"id":10,"name":"guild-update.json","size":1024},{"id":11,"name":"guild-update.json.sig","size":1024},{"id":12,"name":"Guild.zip","size":10},{"id":13,"name":"Guild.zip.sig","size":1024}]})
}
fn server(
    responses: Vec<Vec<u8>>,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
    server_status(responses.into_iter().map(|body| (200, body)).collect())
}
fn server_status(
    responses: Vec<(u16, Vec<u8>)>,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
    server_status_with_headers(
        responses
            .into_iter()
            .map(|(status, body)| (status, body, ""))
            .collect(),
    )
}
fn server_status_with_headers(
    responses: Vec<(u16, Vec<u8>, &'static str)>,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let thread = std::thread::spawn(move || {
        for (status, body, headers) in responses {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 1024];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&chunk[..n]);
                assert!(request.len() <= 16384);
            }
            recorded
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&request).into_owned());
            write!(
                stream,
                "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            let _ = stream.write_all(&body);
        }
    });
    (origin, requests, thread)
}
#[test]
fn public_discovery_authenticates_exact_release_assets_without_gh_or_checkout() {
    let (manifest, bytes, signature, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
    let (origin, requests, thread) = server(vec![
        serde_json::to_vec(&vec![release("nightly-2026-10-18", true)]).unwrap(),
        bytes,
        signature,
    ]);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    let found = provider.discover(Channel::Nightly, &AtomicBool::new(false));
    thread.join().unwrap();
    let found = found.unwrap();
    assert_eq!(found.candidates.len(), 1);
    assert_eq!(found.candidates[0].manifest, manifest);
    assert_eq!(found.candidates[0].archive_id, 12);
    assert_eq!(found.unverified, 0);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].starts_with("GET /repos/owner/guild/releases?per_page=100&page=1 "));
    assert!(requests[1].starts_with("GET /repos/owner/guild/releases/assets/10 "));
    assert!(
        !requests
            .iter()
            .any(|r| r.to_ascii_lowercase().contains("authorization:"))
    );
}
#[test]
fn stable_does_not_fall_back_to_nightly_and_cancelled_check_makes_no_request() {
    let (_, _, _, keys) = fixture("v1.2.0", Channel::Stable);
    let (origin, requests, thread) = server(vec![
        serde_json::to_vec(&vec![release("nightly-2026-10-18", true)]).unwrap(),
    ]);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    assert_eq!(
        provider
            .discover(Channel::Nightly, &AtomicBool::new(true))
            .unwrap_err(),
        Error::Cancelled
    );
    let found = provider.discover(Channel::Stable, &AtomicBool::new(false));
    thread.join().unwrap();
    assert!(found.unwrap().candidates.is_empty());
    assert_eq!(requests.lock().unwrap().len(), 1);
}
#[test]
fn nightly_discovery_paginates_and_reports_incompatible_releases() {
    let (_, bytes, signature, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
    let first:Vec<_>=(1..=100).map(|id|serde_json::json!({"id":id,"tag_name":format!("archive-{id}"),"draft":false,"prerelease":false,"assets":[]})).collect();
    let mut latest = release("nightly-2026-10-18", true);
    latest["id"] = serde_json::json!(101);
    let unsigned = serde_json::json!({"id":102,"tag_name":"nightly-legacy","draft":false,"prerelease":true,"assets":[]});
    let (origin, requests, thread) = server(vec![
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&vec![latest, unsigned]).unwrap(),
        bytes,
        signature,
    ]);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    let found = provider.discover(Channel::Nightly, &AtomicBool::new(false));
    thread.join().unwrap();
    let found = found.unwrap();
    assert_eq!(found.candidates.len(), 1);
    assert_eq!(found.unverified, 1);
    assert!(requests.lock().unwrap()[1].contains("page=2 "));
}
#[test]
fn exhausted_enumeration_never_claims_up_to_date() {
    let (_, _, _, keys) = fixture("v1.2.0", Channel::Stable);
    let pages=(0..10).map(|page|serde_json::to_vec(&(1..=100).map(|i|serde_json::json!({"id":page*100+i,"tag_name":"archive","draft":false,"prerelease":false,"assets":[]})).collect::<Vec<_>>()).unwrap()).collect();
    let (origin, requests, thread) = server(pages);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    let found = provider.discover(Channel::Stable, &AtomicBool::new(false));
    thread.join().unwrap();
    assert_eq!(found.unwrap_err(), Error::Limit);
    assert_eq!(requests.lock().unwrap().len(), 10);
}
#[test]
fn signed_other_repository_or_substituted_metadata_never_becomes_a_candidate() {
    let (mut manifest, _, _, keys) = fixture("v1.2.0", Channel::Stable);
    manifest.repository = "owner/other".into();
    manifest.build.ci.as_mut().unwrap().repository = manifest.repository.clone();
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let sig = update::sign_manifest(&bytes, &SigningKey::from_bytes(&[7; 32]), "fixture").unwrap();
    for signature in [sig, b"invalid detached signature".to_vec()] {
        let (origin, _, thread) = server(vec![
            serde_json::to_vec(&vec![release("v1.2.0", false)]).unwrap(),
            bytes.clone(),
            signature,
        ]);
        let mut provider = Provider::new("owner/guild", keys.clone()).unwrap();
        provider.origin = origin;
        let found = provider.discover(Channel::Stable, &AtomicBool::new(false));
        thread.join().unwrap();
        let found = found.unwrap();
        assert!(found.candidates.is_empty());
        assert_eq!(found.unverified, 1);
    }
}
#[test]
fn stalled_http_body_is_an_idle_timeout_instead_of_a_successful_metadata_read() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let _ = stream.read(&mut request);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n")
            .unwrap();
        stream.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let _ = stream.write_all(b"x");
    });
    let mut provider = Provider::new("owner/guild", KeyRing::new()).unwrap();
    provider.origin = origin;
    let mut bytes = Vec::new();
    let result = provider.http_stream(
        &Request {
            path: "/repos/owner/guild/releases",
            limit: 128,
            asset: false,
            timeout: Duration::from_secs(15),
        },
        &AtomicBool::new(false),
        &mut bytes,
        &mut |_| {},
        &Default::default(),
    );
    server.join().unwrap();
    assert_eq!(result.unwrap_err(), Error::Network);
}
fn fake_gh(body: &[u8], failure: bool) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("body"), body).unwrap();
    let script = if failure {
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/argv\"\nprintf 'fixture-credential-must-not-leak' >&2\nexit 1\n"
    } else {
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/argv\"\nprintf 'HTTP/2.0 200 OK\\r\\nContent-Type: application/json\\r\\n\\r\\n'\ncat \"$(dirname \"$0\")/body\"\nprintf 'fixture-credential-must-not-leak' >&2\n"
    };
    // Fixed scripts only; neither shell source nor args contain production credentials.
    std::fs::write(dir.path().join("gh"), script).unwrap();
    std::fs::set_permissions(
        dir.path().join("gh"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    dir
}
#[test]
fn private_access_uses_explicit_gh_identity_and_public_success_never_invokes_it() {
    for public in [true, false] {
        let fake = fake_gh(b"[]", false);
        let (origin, _, thread) = server_status(vec![(
            if public { 200 } else { 404 },
            if public {
                b"[]".to_vec()
            } else {
                b"private or missing".to_vec()
            },
        )]);
        let mut provider = Provider::new("owner/guild", KeyRing::new()).unwrap();
        provider.origin = origin;
        provider.gh_override = Some(fake.path().join("gh"));
        let result = provider.discover(Channel::Stable, &AtomicBool::new(false));
        thread.join().unwrap();
        assert!(result.unwrap().candidates.is_empty());
        let argv = fake.path().join("argv");
        assert_eq!(argv.exists(), !public);
        if !public {
            let args = std::fs::read_to_string(argv).unwrap();
            assert!(args.contains("--hostname\ngithub.com\n--method\nGET\n"));
            assert!(args.contains("repos/owner/guild/releases?per_page=100&page=1"));
            assert!(!args.contains("auth\ntoken"));
        }
    }
}
#[test]
fn stale_gh_failure_retries_anonymous_without_leaking_diagnostics() {
    let fake = fake_gh(b"", true);
    let (origin, requests, thread) = server_status(vec![
        (404, b"private or missing".to_vec()),
        (200, b"[]".to_vec()),
    ]);
    let mut provider = Provider::new("owner/guild", KeyRing::new()).unwrap();
    provider.origin = origin;
    provider.gh_override = Some(fake.path().join("gh"));
    let result = provider.discover(Channel::Stable, &AtomicBool::new(false));
    thread.join().unwrap();
    assert!(result.unwrap().candidates.is_empty());
    assert_eq!(requests.lock().unwrap().len(), 2);
}

#[test]
fn gh_rate_limits_and_payload_limits_are_sanitized_and_do_not_retry_anonymous() {
    let fake = fake_gh(b"forbidden", false);
    // Keep literal fake headers separate from shell source and raw diagnostic output.
    let script = "#!/bin/sh\ncat \"$(dirname \"$0\")/headers\" \"$(dirname \"$0\")/body\"\nprintf 'fixture-credential-must-not-leak' >&2\n";
    std::fs::write(fake.path().join("gh"), script).unwrap();
    std::fs::write(
        fake.path().join("headers"),
        b"HTTP/2.0 429 Too Many Requests\r\nRetry-After: 10\r\n\r\n",
    )
    .unwrap();
    let request = Request {
        path: "/repos/owner/guild/releases",
        limit: 128,
        asset: false,
        timeout: Duration::from_secs(3),
    };
    let mut output = Vec::new();
    let before = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let error = gh::request(
        &request,
        &AtomicBool::new(false),
        &mut output,
        &mut |_| {},
        Some(&fake.path().join("gh")),
        &Default::default(),
    )
    .unwrap_err();
    assert!(matches!(error,Error::RateLimited(at) if at>=before+10));
    assert!(output.is_empty());
    assert!(!format!("{error:?}").contains("fixture-credential"));
    std::fs::write(fake.path().join("headers"), b"HTTP/2.0 200 OK\r\n\r\n").unwrap();
    std::fs::write(fake.path().join("body"), vec![b'x'; 65536]).unwrap();
    assert_eq!(
        gh::request(
            &request,
            &AtomicBool::new(false),
            &mut output,
            &mut |_| {},
            Some(&fake.path().join("gh")),
            &Default::default(),
        )
        .unwrap_err(),
        Error::Limit
    );
}
#[test]
fn cancelling_started_gh_stops_the_owned_group_before_descendants_can_publish() {
    let fake = fake_gh(b"", false);
    let started = fake.path().join("started");
    let survived = fake.path().join("survived");
    let script = r#"#!/bin/sh
fixture_dir=$(dirname "$0")
printf 'HTTP/2.0 200 OK\r\n\r\n'
touch "$fixture_dir/started"
(sleep 2; touch "$fixture_dir/survived") &
wait
"#;
    std::fs::write(fake.path().join("gh"), script).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel = cancelled.clone();
    let cancellation = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !started.exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        cancel.store(true, Ordering::Release);
    });
    let request = Request {
        path: "/repos/owner/guild/releases/assets/12",
        limit: 1024,
        asset: true,
        timeout: Duration::from_secs(5),
    };
    let result = gh::request(
        &request,
        &cancelled,
        &mut Vec::new(),
        &mut |_| {},
        Some(&fake.path().join("gh")),
        &Default::default(),
    );
    cancellation.join().unwrap();
    assert_eq!(result.unwrap_err(), Error::Cancelled);
    std::thread::sleep(Duration::from_millis(2100));
    assert!(!survived.exists());
}

#[test]
fn github_retry_after_http_date_is_honored() {
    let fake = fake_gh(b"", false);
    std::fs::write(
        fake.path().join("gh"),
        "#!/bin/sh\ncat \"$(dirname \"$0\")/headers\"\n",
    )
    .unwrap();
    std::fs::write(
        fake.path().join("headers"),
        b"HTTP/2.0 429 Too Many Requests\r\nRetry-After: Thu, 01 Jan 2099 00:00:00 GMT\r\n\r\n",
    )
    .unwrap();
    let request = Request {
        path: "/repos/owner/guild/releases",
        limit: 128,
        asset: false,
        timeout: Duration::from_secs(3),
    };
    let error = gh::request(
        &request,
        &AtomicBool::new(false),
        &mut Vec::new(),
        &mut |_| {},
        Some(&fake.path().join("gh")),
        &Default::default(),
    )
    .unwrap_err();
    assert!(matches!(error,Error::RateLimited(at) if at>=4_070_908_800));
}

#[test]
fn private_not_found_stays_ambiguous_and_missing_gh_is_distinct() {
    for missing in [true, false] {
        let fake = fake_gh(b"", true);
        let (origin, requests, thread) = server_status(vec![
            (404, b"fixture-credential-must-not-leak".to_vec()),
            (404, b"fixture-credential-must-not-leak".to_vec()),
        ]);
        let mut provider = Provider::new("owner/guild", KeyRing::new()).unwrap();
        provider.origin = origin;
        provider.gh_override = Some(fake.path().join(if missing { "missing-gh" } else { "gh" }));
        let result = provider.discover(Channel::Stable, &AtomicBool::new(false));
        thread.join().unwrap();
        let error = result.unwrap_err();
        assert_eq!(
            error,
            if missing {
                Error::MissingGh
            } else {
                Error::Access
            }
        );
        assert!(!format!("{error:?}").contains("fixture-credential"));
        assert_eq!(requests.lock().unwrap().len(), 2);
    }
}
#[test]
fn provider_does_not_retry_through_rate_limit_or_accept_wrong_gh_response_kind() {
    for limited in [true, false] {
        let fake = fake_gh(b"{}", false);
        if limited {
            std::fs::write(fake.path().join("gh"),"#!/bin/sh\nprintf 'HTTP/2.0 429 Too Many Requests\\r\\nRetry-After: 60\\r\\n\\r\\n'\n").unwrap();
        }
        let (origin, requests, thread) = server_status(vec![(404, b"private or missing".to_vec())]);
        let mut provider = Provider::new("owner/guild", KeyRing::new()).unwrap();
        provider.origin = origin;
        provider.gh_override = Some(fake.path().join("gh"));
        let result = provider.discover(Channel::Stable, &AtomicBool::new(false));
        thread.join().unwrap();
        let error = result.unwrap_err();
        assert!(if limited {
            matches!(error, Error::RateLimited(_))
        } else {
            error == Error::Invalid
        });
        assert_eq!(requests.lock().unwrap().len(), 1);
    }
}

#[cfg(target_os = "macos")]
mod download;

/// Native bundle bytes shared with Root integration regressions; never executed.
#[cfg(target_os = "macos")]
pub(crate) fn native_bundle_fixture() -> tempfile::TempDir {
    download::signed_payload_fixture(false).root
}

#[test]
fn discovery_overlaps_metadata_and_orders_only_authenticated_revisions() {
    let (mut older, _, _, keys) = fixture("nightly-2026-10-26", Channel::Nightly);
    older.build.revision_order = Some(49);
    let (newer, _, _, _) = fixture("nightly-2026-10-24", Channel::Nightly);
    let signed = |manifest: &Manifest| {
        let bytes = serde_json::to_vec(manifest).unwrap();
        let sig =
            update::sign_manifest(&bytes, &SigningKey::from_bytes(&[7; 32]), "fixture").unwrap();
        (bytes, sig)
    };
    let (old_bytes, old_sig) = signed(&older);
    let (new_bytes, new_sig) = signed(&newer);
    let mut first = release("nightly-2026-10-26", true);
    let mut second = release("nightly-2026-10-24", true);
    second["id"] = 2.into();
    for asset in second["assets"].as_array_mut().unwrap() {
        asset["id"] = (asset["id"].as_u64().unwrap() + 10).into();
    }
    // Unsigned newest release must not hide a valid fallback.
    first["id"] = 1.into();
    let invalid = serde_json::json!({"id":3,"tag_name":"nightly-2026-10-27","prerelease":true,"draft":false,"assets":[]});
    let listing = serde_json::to_vec(&vec![invalid, first, second]).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let receive = || {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut request = vec![];
                        while !request.windows(4).any(|b| b == b"\r\n\r\n") {
                            let mut buf = [0; 1024];
                            let n = stream.read(&mut buf).unwrap();
                            assert!(n > 0);
                            request.extend_from_slice(&buf[..n]);
                        }
                        return Some((stream, String::from_utf8(request).unwrap()));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return None;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        };
        let respond = |mut stream: std::net::TcpStream, body: &[u8]| {
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            let _ = stream.write_all(body);
        };
        respond(receive().unwrap().0, &listing);
        // Hold both manifest requests before responding: serialized discovery cannot pass.
        let a = receive().unwrap();
        let Some(b) = receive() else {
            return;
        };
        for (stream, request) in [a, b] {
            respond(
                stream,
                if request.contains("/assets/10 ") {
                    &old_bytes
                } else {
                    &new_bytes
                },
            );
        }
        for _ in 0..2 {
            let (stream, request) = receive().unwrap();
            respond(
                stream,
                if request.contains("/assets/11 ") {
                    &old_sig
                } else {
                    &new_sig
                },
            );
        }
    });
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    let found = provider.discover(Channel::Nightly, &AtomicBool::new(false));
    thread.join().unwrap();
    let found = found.expect("metadata verification must overlap while retaining signed ordering");
    assert_eq!(found.unverified, 1);
    assert_eq!(
        found
            .candidates
            .iter()
            .map(|c| c.manifest.build.revision_order.unwrap())
            .collect::<Vec<_>>(),
        vec![50, 49]
    );
}

fn digested_release(bytes: &[u8], signature: &[u8]) -> serde_json::Value {
    use sha2::{Digest, Sha256};
    let mut value = release("nightly-2026-10-18", true);
    for (asset, bytes) in [(0, bytes), (1, signature)] {
        value["assets"][asset]["digest"] = format!("sha256:{:x}", Sha256::digest(bytes)).into();
        value["assets"][asset]["updated_at"] = "2026-11-02T00:00:00Z".into();
    }
    value
}

#[test]
fn repeated_discovery_reuses_verified_metadata_but_refreshes_listing() {
    let (_, bytes, sig, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
    let listing = serde_json::to_vec(&vec![digested_release(&bytes, &sig)]).unwrap();
    let (origin, requests, thread) = server(vec![listing.clone(), bytes, sig, listing]);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    for _ in 0..2 {
        let found = provider
            .discover(Channel::Nightly, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(found.candidates.len(), 1);
    }
    thread.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 4);
}

#[test]
fn anonymous_throttle_uses_gh_once_and_preserves_terminal_throttling() {
    for mode in ["authorized", "unauthorized", "missing"] {
        let fake = fake_gh(b"[]", mode == "unauthorized");
        let (origin, requests, thread) = server_status(vec![(429, b"secret diagnostic".to_vec())]);
        let mut provider = Provider::new("owner/guild", KeyRing::new()).unwrap();
        provider.origin = origin;
        provider.gh_override = Some(fake.path().join(if mode == "missing" {
            "missing-gh"
        } else {
            "gh"
        }));
        let found = provider.discover(Channel::Stable, &AtomicBool::new(false));
        thread.join().unwrap();
        if mode == "authorized" {
            assert!(found.unwrap().candidates.is_empty());
        } else {
            assert!(matches!(found.unwrap_err(), Error::RateLimited(_)));
        }
        assert_eq!(fake.path().join("argv").exists(), mode != "missing");
        assert_eq!(requests.lock().unwrap().len(), 1);
    }
}

#[test]
fn metadata_reuse_invalidates_on_every_release_and_asset_identity_change() {
    let (_, bytes, sig, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
    let original = digested_release(&bytes, &sig);
    let mut changes = Vec::new();
    for (pointer, value) in [
        ("/id", serde_json::json!(2)),
        ("/updated_at", serde_json::json!("2026-11-02T01:00:00Z")),
        ("/assets/0/id", serde_json::json!(20)),
        ("/assets/1/id", serde_json::json!(21)),
        (
            "/assets/0/updated_at",
            serde_json::json!("2026-11-02T01:00:00Z"),
        ),
        ("/assets/1/digest", serde_json::json!("sha256:replaced")),
        ("/assets/2/id", serde_json::json!(22)),
        ("/assets/3/id", serde_json::json!(23)),
        ("/assets/0/digest", serde_json::Value::Null),
    ] {
        let mut changed = original.clone();
        // Release updated_at is absent in the original fixture.
        if pointer == "/updated_at" {
            changed["updated_at"] = value;
        } else {
            *changed.pointer_mut(pointer).unwrap() = value;
        }
        changes.push(changed);
    }
    for changed in changes {
        let (origin, requests, thread) = server(vec![
            serde_json::to_vec(&vec![original.clone()]).unwrap(),
            bytes.clone(),
            sig.clone(),
            serde_json::to_vec(&vec![changed]).unwrap(),
            bytes.clone(),
            sig.clone(),
        ]);
        let mut provider = Provider::new("owner/guild", keys.clone()).unwrap();
        provider.origin = origin;
        for _ in 0..2 {
            assert_eq!(
                provider
                    .discover(Channel::Nightly, &AtomicBool::new(false))
                    .unwrap()
                    .candidates
                    .len(),
                1
            );
        }
        thread.join().unwrap();
        assert_eq!(requests.lock().unwrap().len(), 6);
    }
}

#[test]
fn replaced_bad_signature_never_reuses_old_authority_or_caches_failure() {
    let (_, bytes, sig, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
    let original = digested_release(&bytes, &sig);
    let bad = b"invalid signature".to_vec();
    let replaced = digested_release(&bytes, &bad);
    let mut responses = vec![
        serde_json::to_vec(&vec![original]).unwrap(),
        bytes.clone(),
        sig,
    ];
    for _ in 0..2 {
        responses.extend([
            serde_json::to_vec(&vec![replaced.clone()]).unwrap(),
            bytes.clone(),
            bad.clone(),
        ]);
    }
    let (origin, requests, thread) = server(responses);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    assert_eq!(
        provider
            .discover(Channel::Nightly, &AtomicBool::new(false))
            .unwrap()
            .candidates
            .len(),
        1
    );
    for _ in 0..2 {
        let found = provider
            .discover(Channel::Nightly, &AtomicBool::new(false))
            .unwrap();
        assert!(found.candidates.is_empty());
        assert_eq!(found.unverified, 1);
    }
    thread.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 9);
}

#[test]
fn reuse_still_requires_current_trusted_signature_keys() {
    let (_, bytes, sig, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
    let listing = serde_json::to_vec(&vec![digested_release(&bytes, &sig)]).unwrap();
    let (origin, requests, thread) = server(vec![listing.clone(), bytes, sig, listing]);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    assert_eq!(
        provider
            .discover(Channel::Nightly, &AtomicBool::new(false))
            .unwrap()
            .candidates
            .len(),
        1
    );
    provider.keys.clear();
    let found = provider
        .discover(Channel::Nightly, &AtomicBool::new(false))
        .unwrap();
    assert!(found.candidates.is_empty());
    assert_eq!(found.unverified, 1);
    thread.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 4);
}

#[test]
fn gh_throttle_after_anonymous_throttle_never_loops() {
    let fake = fake_gh(b"", false);
    std::fs::write(fake.path().join("gh"), "#!/bin/sh\necho call >> \"$(dirname \"$0\")/calls\"\nprintf 'HTTP/2.0 429 Too Many Requests\\r\\nRetry-After: 3600\\r\\n\\r\\n'\nexit 1\n").unwrap();
    let (origin, requests, thread) = server_status(vec![(429, vec![])]);
    let mut provider = Provider::new("owner/guild", KeyRing::new()).unwrap();
    provider.origin = origin;
    provider.gh_override = Some(fake.path().join("gh"));
    let before = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(
        matches!(provider.discover(Channel::Stable, &AtomicBool::new(false)), Err(Error::RateLimited(at)) if at >= before + 3600)
    );
    thread.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert_eq!(
        std::fs::read_to_string(fake.path().join("calls")).unwrap(),
        "call\n"
    );
}

#[test]
fn releases_without_digests_are_fetched_and_verified_on_every_check() {
    let (_, bytes, sig, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
    let listing = serde_json::to_vec(&vec![release("nightly-2026-10-18", true)]).unwrap();
    let (origin, requests, thread) = server(vec![
        listing.clone(),
        bytes.clone(),
        sig.clone(),
        listing,
        bytes,
        sig,
    ]);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    for _ in 0..2 {
        assert_eq!(
            provider
                .discover(Channel::Nightly, &AtomicBool::new(false))
                .unwrap()
                .candidates
                .len(),
            1
        );
    }
    thread.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 6);
}

#[test]
fn fresh_listing_removal_never_offers_retained_metadata() {
    let (_, bytes, sig, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
    let listing = serde_json::to_vec(&vec![digested_release(&bytes, &sig)]).unwrap();
    let (origin, requests, thread) = server(vec![listing, bytes, sig, b"[]".to_vec()]);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    assert_eq!(
        provider
            .discover(Channel::Nightly, &AtomicBool::new(false))
            .unwrap()
            .candidates
            .len(),
        1
    );
    assert!(
        provider
            .discover(Channel::Nightly, &AtomicBool::new(false))
            .unwrap()
            .candidates
            .is_empty()
    );
    thread.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 4);
}

#[test]
fn later_page_throttle_restarts_authenticated_listing_before_draft_boundary_shift() {
    let (_, bytes, sig, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
    let published: Vec<_> = (1..=100).map(|id| serde_json::json!({
        "id":id,"tag_name":format!("archive-{id}"),"draft":false,"prerelease":false,"assets":[]
    })).collect();
    let mut authenticated = vec![serde_json::json!({
        "id":999,"tag_name":"nightly-draft","draft":true,"prerelease":true,"assets":[]
    })];
    authenticated.extend(published[..99].iter().cloned());
    let mut nightly = release("nightly-2026-10-18", true);
    nightly["id"] = 101.into();
    let fake = fake_gh(b"", false);
    std::fs::write(
        fake.path().join("page1"),
        serde_json::to_vec(&authenticated).unwrap(),
    )
    .unwrap();
    std::fs::write(
        fake.path().join("page2"),
        serde_json::to_vec(&vec![published[99].clone(), nightly]).unwrap(),
    )
    .unwrap();
    std::fs::write(
        fake.path().join("gh"),
        r#"#!/bin/sh
fixture_dir=$(dirname "$0")
for argument in "$@"; do endpoint=$argument; done
printf '%s\n' "$endpoint" >> "$fixture_dir/calls"
printf 'HTTP/2.0 200 OK\r\nContent-Type: application/json\r\n\r\n'
case "$endpoint" in
 *'page=1') cat "$fixture_dir/page1";;
 *'page=2') cat "$fixture_dir/page2";;
 *) exit 1;;
esac
"#,
    )
    .unwrap();
    let (origin, requests, thread) = server_status(vec![
        (200, serde_json::to_vec(&published).unwrap()),
        (429, vec![]),
        (200, bytes),
        (200, sig),
    ]);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    provider.gh_override = Some(fake.path().join("gh"));
    let found = provider.discover(Channel::Nightly, &AtomicBool::new(false));
    thread.join().unwrap();
    let found = found.expect("authenticated enumeration must restart at page 1");
    assert_eq!(found.candidates.len(), 1);
    assert_eq!(found.candidates[0].release_id, 101);
    assert_eq!(
        found.unverified, 0,
        "drafts never become unsigned candidate warnings"
    );
    assert_eq!(
        std::fs::read_to_string(fake.path().join("calls")).unwrap(),
        "repos/owner/guild/releases?per_page=100&page=1\nrepos/owner/guild/releases?per_page=100&page=2\n"
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert!(requests[0].contains("page=1 "));
    assert!(requests[1].contains("page=2 "));
    assert!(requests[2].contains("/assets/10 "));
}

#[test]
fn authenticated_restart_keeps_ten_page_bound_and_never_returns_partial_rows() {
    let fake = fake_gh(b"", false);
    for page in 1..=10 {
        let rows: Vec<_> = (1..=100).map(|offset| serde_json::json!({
            "id":(page-1)*100+offset,"tag_name":"archive","draft":false,"prerelease":false,"assets":[]
        })).collect();
        std::fs::write(
            fake.path().join(format!("page{page}")),
            serde_json::to_vec(&rows).unwrap(),
        )
        .unwrap();
    }
    std::fs::write(
        fake.path().join("gh"),
        r#"#!/bin/sh
fixture_dir=$(dirname "$0")
for argument in "$@"; do endpoint=$argument; done
printf '%s\n' "$endpoint" >> "$fixture_dir/calls"
printf 'HTTP/2.0 200 OK\r\nContent-Type: application/json\r\n\r\n'
cat "$fixture_dir/page${endpoint##*=}"
"#,
    )
    .unwrap();
    let (origin, requests, thread) = server_status(vec![(429, vec![])]);
    let mut provider = Provider::new("owner/guild", KeyRing::new()).unwrap();
    provider.origin = origin;
    provider.gh_override = Some(fake.path().join("gh"));
    assert_eq!(
        provider
            .discover(Channel::Nightly, &AtomicBool::new(false))
            .unwrap_err(),
        Error::Limit
    );
    thread.join().unwrap();
    let calls = std::fs::read_to_string(fake.path().join("calls")).unwrap();
    assert_eq!(calls.lines().count(), 10);
    assert!(calls.lines().next().unwrap().ends_with("page=1"));
    assert!(calls.lines().last().unwrap().ends_with("page=10"));
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[test]
fn authenticated_restart_uses_original_deadline_and_cancellation() {
    let fake = fake_gh(b"[]", false);
    std::fs::write(
        fake.path().join("gh"),
        r#"#!/bin/sh
fixture_dir=$(dirname "$0")
touch "$fixture_dir/started"
sleep 10
printf 'HTTP/2.0 200 OK\r\nContent-Type: application/json\r\n\r\n[]'
"#,
    )
    .unwrap();
    let (origin, requests, thread) = server_status(vec![(429, vec![])]);
    let mut provider = Provider::new("owner/guild", KeyRing::new()).unwrap();
    provider.origin = origin;
    provider.gh_override = Some(fake.path().join("gh"));
    let started = Instant::now();
    let error = provider
        .list_releases(
            Channel::Nightly,
            &AtomicBool::new(false),
            started + Duration::from_secs(2),
        )
        .err()
        .unwrap();
    thread.join().unwrap();
    assert_eq!(error, Error::Network);
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "transport switch must not reset discovery deadline"
    );
    assert!(fake.path().join("started").exists());
    assert_eq!(requests.lock().unwrap().len(), 1);
    std::fs::remove_file(fake.path().join("started")).unwrap();
    let error = provider
        .list_with_transport(
            Channel::Nightly,
            &AtomicBool::new(true),
            Instant::now() + Duration::from_secs(1),
            ListingTransport::Authorized,
        )
        .err()
        .unwrap();
    assert_eq!(error, Error::Cancelled);
    assert!(
        !fake.path().join("started").exists(),
        "cancelled pass cannot start a gh process"
    );
}

#[test]
fn later_authenticated_throttle_does_not_fall_back_or_publish_partial_listing() {
    let fake = fake_gh(b"", false);
    let first: Vec<_> = (1..=100)
        .map(|id| {
            serde_json::json!({
                "id":id,"tag_name":"archive","draft":false,"prerelease":false,"assets":[]
            })
        })
        .collect();
    std::fs::write(
        fake.path().join("page1"),
        serde_json::to_vec(&first).unwrap(),
    )
    .unwrap();
    std::fs::write(fake.path().join("gh"), r#"#!/bin/sh
fixture_dir=$(dirname "$0")
for argument in "$@"; do endpoint=$argument; done
printf '%s\n' "$endpoint" >> "$fixture_dir/calls"
case "$endpoint" in
 *'page=1') printf 'HTTP/2.0 200 OK\r\nContent-Type: application/json\r\n\r\n'; cat "$fixture_dir/page1";;
 *'page=2') printf 'HTTP/2.0 429 Too Many Requests\r\nRetry-After: 3600\r\n\r\n'; exit 1;;
 *) exit 1;;
esac
"#).unwrap();
    let (origin, requests, thread) = server_status(vec![(404, vec![])]);
    let mut provider = Provider::new("owner/guild", KeyRing::new()).unwrap();
    provider.origin = origin;
    provider.gh_override = Some(fake.path().join("gh"));
    let before = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(
        matches!(provider.discover(Channel::Nightly, &AtomicBool::new(false)), Err(Error::RateLimited(at)) if at >= before + 3600)
    );
    thread.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert_eq!(
        std::fs::read_to_string(fake.path().join("calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[test]
fn metadata_asset_throttling_uses_exact_gh_asset_and_still_verifies_candidate() {
    for limited_asset in [10, 11] {
        let (manifest, bytes, sig, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
        let fake = fake_gh(b"", false);
        std::fs::write(fake.path().join("manifest"), &bytes).unwrap();
        std::fs::write(fake.path().join("signature"), &sig).unwrap();
        std::fs::write(
            fake.path().join("gh"),
            r#"#!/bin/sh
fixture_dir=$(dirname "$0")
for argument in "$@"; do endpoint=$argument; done
printf '%s\n' "$endpoint" >> "$fixture_dir/calls"
printf '%s\n' "$@" >> "$fixture_dir/argv"
printf 'HTTP/2.0 200 OK\r\nContent-Type: application/octet-stream\r\n\r\n'
case "$endpoint" in
 repos/owner/guild/releases/assets/10) cat "$fixture_dir/manifest";;
 repos/owner/guild/releases/assets/11) cat "$fixture_dir/signature";;
 *) exit 1;;
esac
"#,
        )
        .unwrap();
        let (origin, requests, thread) = server_status(vec![
            (
                200,
                serde_json::to_vec(&vec![release("nightly-2026-10-18", true)]).unwrap(),
            ),
            (if limited_asset == 10 { 429 } else { 200 }, bytes),
            (if limited_asset == 11 { 429 } else { 200 }, sig),
        ]);
        let mut provider = Provider::new("owner/guild", keys).unwrap();
        provider.origin = origin;
        provider.gh_override = Some(fake.path().join("gh"));
        let found = provider.discover(Channel::Nightly, &AtomicBool::new(false));
        thread.join().unwrap();
        let found =
            found.expect("asset throttling must recover independently of listing transport");
        assert_eq!(found.candidates.len(), 1);
        assert_eq!(found.candidates[0].manifest, manifest);
        assert_eq!(found.candidates[0].release_id, 1);
        assert_eq!(found.candidates[0].archive_id, 12);
        assert_eq!(found.candidates[0].signature_id, 13);
        assert_eq!(found.unverified, 0);
        assert_eq!(
            std::fs::read_to_string(fake.path().join("calls")).unwrap(),
            format!("repos/owner/guild/releases/assets/{limited_asset}\n")
        );
        let argv = std::fs::read_to_string(fake.path().join("argv")).unwrap();
        assert!(argv.contains("--hostname\ngithub.com\n--method\nGET\n"));
        assert!(argv.contains("Accept: application/octet-stream"));
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].contains("releases?per_page=100&page=1 "));
        assert!(requests[1].contains("/releases/assets/10 "));
        assert!(requests[2].contains("/releases/assets/11 "));
    }
}

#[test]
fn metadata_asset_throttling_without_authorized_gh_keeps_original_reset_without_loop() {
    const RESET: u64 = 4_070_908_800;
    const HEADERS: &str = "X-RateLimit-Remaining: 0\r\nX-RateLimit-Reset: 4070908800\r\n";
    for limited_asset in [10, 11] {
        for unavailable in ["missing", "unauthorized"] {
            let (_, bytes, _, keys) = fixture("nightly-2026-10-18", Channel::Nightly);
            let fake = fake_gh(b"", true);
            let mut responses = vec![(
                200,
                serde_json::to_vec(&vec![release("nightly-2026-10-18", true)]).unwrap(),
                "",
            )];
            if limited_asset == 11 {
                responses.push((200, bytes, ""));
            }
            responses.push((429, b"fixture-credential-must-not-leak".to_vec(), HEADERS));
            let (origin, requests, thread) = server_status_with_headers(responses);
            let mut provider = Provider::new("owner/guild", keys).unwrap();
            provider.origin = origin;
            provider.gh_override = Some(fake.path().join(if unavailable == "missing" {
                "missing-gh"
            } else {
                "gh"
            }));
            let error = provider
                .discover(Channel::Nightly, &AtomicBool::new(false))
                .unwrap_err();
            thread.join().unwrap();
            assert_eq!(error, Error::RateLimited(RESET));
            assert!(!format!("{error:?}").contains("fixture-credential"));
            let requests = requests.lock().unwrap();
            assert_eq!(requests.len(), if limited_asset == 10 { 2 } else { 3 });
            assert!(requests[0].contains("releases?per_page=100&page=1 "));
            assert!(
                requests
                    .last()
                    .unwrap()
                    .contains(&format!("/releases/assets/{limited_asset} "))
            );
            let argv = fake.path().join("argv");
            assert_eq!(argv.exists(), unavailable == "unauthorized");
            if argv.exists() {
                let argv = std::fs::read_to_string(argv).unwrap();
                assert_eq!(argv.lines().filter(|line| *line == "api").count(), 1);
                assert!(argv.ends_with(&format!(
                    "repos/owner/guild/releases/assets/{limited_asset}\n"
                )));
            }
        }
    }
}
