use super::*;
use std::path::Path;
#[cfg(target_os = "macos")]
pub(super) struct PayloadFixture {
    pub(super) root: tempfile::TempDir,
    manifest: Manifest,
    metadata: Vec<u8>,
    metadata_sig: Vec<u8>,
    payload: Vec<u8>,
    payload_sig: Vec<u8>,
    keys: KeyRing,
}
pub(super) fn signed_payload_fixture(wrong_cli_arch: bool) -> PayloadFixture {
    use guild_session::update::bundle;
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("Guild.app");
    let resources = app.join("Contents/Resources");
    let executables = app.join("Contents/MacOS");
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::create_dir(&executables).unwrap();
    let (mut manifest, _, _, keys) = fixture("v1.2.0", Channel::Stable);
    std::fs::write(
        resources.join("build-info.json"),
        serde_json::to_vec(&manifest.build).unwrap(),
    )
    .unwrap();
    std::fs::write(
        resources.join("commit"),
        format!("{}\n", manifest.build.commit),
    )
    .unwrap();
    std::fs::write(resources.join("tag"), "v1.2.0\n").unwrap();
    std::fs::write(resources.join("built"), "0\n").unwrap();
    let public: BTreeMap<_, _> = keys
        .iter()
        .map(|(id, key)| (id.clone(), update::hex(key)))
        .collect();
    std::fs::write(
        resources.join("update-keys.json"),
        serde_json::to_vec(&public).unwrap(),
    )
    .unwrap();
    std::fs::write(app.join("Contents/Info.plist"),r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>dev.guild.desktop</string><key>CFBundleExecutable</key><string>guild-desktop</string><key>CFBundlePackageType</key><string>APPL</string><key>LSMinimumSystemVersion</key><string>13.0</string></dict></plist>"#).unwrap();
    let code = root.path().join("fixture.c");
    std::fs::write(&code, "int main(void) { return 0; }\n").unwrap();
    for name in ["guild-desktop", "guild"] {
        let arch = if name == "guild" && wrong_cli_arch {
            "x86_64"
        } else {
            "arm64"
        };
        let out = Command::new("/usr/bin/clang")
            .args(["-arch", arch, "-mmacosx-version-min=13.0"])
            .arg(&code)
            .arg("-o")
            .arg(executables.join(name))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let out = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(out.status.success());
    let zip = root.path().join("Guild.zip");
    let out = Command::new("/usr/bin/ditto")
        .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
        .arg(&app)
        .arg(&zip)
        .output()
        .unwrap();
    assert!(out.status.success());
    (manifest.bytes, manifest.sha256) = bundle::hash_file(&zip, &AtomicBool::new(false)).unwrap();
    let key = SigningKey::from_bytes(&[7; 32]);
    let payload = std::fs::read(zip).unwrap();
    let archive_sig = update::sign_archive(payload.as_slice(), &key, "fixture").unwrap();
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let sig = update::sign_manifest(&bytes, &key, "fixture").unwrap();
    PayloadFixture {
        root,
        manifest,
        metadata: bytes,
        metadata_sig: sig,
        payload,
        payload_sig: archive_sig,
        keys,
    }
}
#[test]
#[cfg(target_os = "macos")]
fn public_payload_download_authenticates_and_validates_before_exposing_an_owned_app() {
    let PayloadFixture {
        root,
        manifest,
        metadata,
        metadata_sig,
        payload,
        payload_sig,
        keys,
    } = signed_payload_fixture(false);
    let mut summary = release("v1.2.0", false);
    summary["assets"][2]["size"] = serde_json::json!(manifest.bytes);
    let (origin, _, thread) = server(vec![
        serde_json::to_vec(&vec![summary]).unwrap(),
        metadata,
        metadata_sig,
        payload_sig,
        payload,
    ]);
    let mut provider = Provider::new("owner/guild", keys).unwrap();
    provider.origin = origin;
    let cancel = AtomicBool::new(false);
    let candidate = provider
        .discover(Channel::Stable, &cancel)
        .unwrap()
        .candidates
        .remove(0);
    let cache = root.path().join("cache");
    let mut progress = Vec::new();
    let result = provider.download(&candidate, &cache, &cancel, &mut |event| {
        progress.push(event)
    });
    thread.join().unwrap();
    let acquired = result.unwrap();
    assert_eq!(
        guild_session::update::bundle::info(acquired.bundle()).unwrap(),
        manifest.build
    );
    assert!(progress.contains(&Progress::Downloading {
        bytes: manifest.bytes,
        total: manifest.bytes
    }));
    assert_eq!(progress.last(), Some(&Progress::Verifying));
    let path = acquired.work_dir().to_path_buf();
    drop(acquired);
    assert!(
        !path.exists(),
        "unaccepted downloads are removed by their owner"
    );
}

fn serve_payload(
    fixture: &PayloadFixture,
    payload: Vec<u8>,
) -> (Provider, std::thread::JoinHandle<()>) {
    let mut summary = release("v1.2.0", false);
    summary["assets"][2]["size"] = serde_json::json!(fixture.manifest.bytes);
    let (origin, _, thread) = server(vec![
        serde_json::to_vec(&vec![summary]).unwrap(),
        fixture.metadata.clone(),
        fixture.metadata_sig.clone(),
        fixture.payload_sig.clone(),
        payload,
    ]);
    let mut provider = Provider::new("owner/guild", fixture.keys.clone()).unwrap();
    provider.origin = origin;
    (provider, thread)
}
#[test]
fn corrupt_incomplete_oversized_and_wrong_architecture_downloads_never_leave_candidates() {
    for case in ["corrupt", "incomplete", "oversized", "wrong-architecture"] {
        let fixture = signed_payload_fixture(case == "wrong-architecture");
        let mut payload = fixture.payload.clone();
        match case {
            "corrupt" => payload[0] ^= 1,
            "incomplete" => {
                payload.pop();
            }
            "oversized" => payload.push(0),
            _ => {}
        }
        let (provider, server) = serve_payload(&fixture, payload);
        let cancel = AtomicBool::new(false);
        let candidate = provider
            .discover(Channel::Stable, &cancel)
            .unwrap()
            .candidates
            .remove(0);
        let cache = fixture.root.path().join("cache");
        let result = provider.download(&candidate, &cache, &cancel, &mut |_| {});
        server.join().unwrap();
        assert_eq!(
            result.unwrap_err(),
            if case == "oversized" {
                Error::Limit
            } else {
                Error::Verification
            },
            "{case}"
        );
        assert!(
            std::fs::read_dir(cache).unwrap().next().is_none(),
            "{case}: owned partial output is removed"
        );
    }
}
#[test]
fn source_switch_cancels_stream_and_late_channel_result_cannot_be_published() {
    use crate::update::control::{Gate, Identity, Origin};
    use guild_session::update::{Preferences, Source};
    for cancel_work in [true, false] {
        let fixture = signed_payload_fixture(false);
        let (provider, server) = serve_payload(&fixture, fixture.payload.clone());
        let cancel = AtomicBool::new(false);
        let candidate = provider
            .discover(Channel::Stable, &cancel)
            .unwrap()
            .candidates
            .remove(0);
        let preferences = Preferences {
            channel: Channel::Stable,
            ..Preferences::default()
        };
        let mut gate = Gate::pending(preferences.clone(), "owner/guild".into());
        gate.acknowledge(gate.selection(), &preferences);
        let ticket = gate.begin().unwrap();
        let identity = Identity {
            artifact: crate::update::control::Artifact::Build(candidate.manifest.build.build_id),
            origin: Origin::Remote {
                repository: "owner/guild".into(),
                channel: Channel::Stable,
                release_id: candidate.release_id,
                sha256: candidate.manifest.sha256.clone(),
            },
        };
        assert!(gate.accept(&ticket, identity.clone()));
        let cache = fixture.root.path().join("cache");
        let mut switched = false;
        let result = provider.download(&candidate, &cache, &cancel, &mut |event| {
            if !switched && matches!(event,Progress::Downloading{bytes,..} if bytes>0) {
                switched = true;
                if cancel_work {
                    gate.change(Preferences {
                        source: Source::Local,
                        ..preferences.clone()
                    });
                    cancel.store(true, Ordering::Release);
                } else {
                    gate.change(Preferences {
                        channel: Channel::Nightly,
                        ..preferences.clone()
                    });
                }
            }
        });
        server.join().unwrap();
        assert!(switched);
        assert!(!gate.owns(&ticket));
        assert!(!gate.accept(&ticket, identity.clone()));
        if cancel_work {
            assert_eq!(result.unwrap_err(), Error::Cancelled);
        } else {
            // Simulate a worker that finished after cancellation was requested.
            let acquired = result.unwrap();
            let path = acquired.work_dir().to_path_buf();
            drop(acquired);
            assert!(!path.exists());
            let selected = gate.preferences().clone();
            gate.acknowledge(gate.selection(), &selected);
            let latest = gate.begin().unwrap();
            assert!(!gate.accept(&latest, identity));
        }
        assert!(std::fs::read_dir(cache).unwrap().next().is_none());
    }
}

#[test]
fn ready_cache_rechecks_signed_bytes_and_scope_after_restart_and_rejects_late_publication() {
    use crate::update::control::{Gate, Identity, Origin};
    use guild_session::update::Preferences;
    let fixture = signed_payload_fixture(false);
    let (provider, server) = serve_payload(&fixture, fixture.payload.clone());
    let cancel = AtomicBool::new(false);
    let candidate = provider
        .discover(Channel::Stable, &cancel)
        .unwrap()
        .candidates
        .remove(0);
    let root = fixture.root.path().join("persistent");
    let work = fixture.root.path().join("work");
    let acquired = provider
        .download(&candidate, &work, &cancel, &mut |_| {})
        .unwrap();
    server.join().unwrap();
    let preferences = Preferences {
        channel: Channel::Stable,
        ..Preferences::default()
    };
    let mut gate = Gate::pending(preferences.clone(), "owner/guild".into());
    gate.acknowledge(gate.selection(), &preferences);
    let ticket = gate.begin().unwrap();
    let identity = Identity {
        artifact: crate::update::control::Artifact::Build(candidate.manifest.build.build_id),
        origin: Origin::Remote {
            repository: "owner/guild".into(),
            channel: Channel::Stable,
            release_id: candidate.release_id,
            sha256: candidate.manifest.sha256.clone(),
        },
    };
    assert!(gate.accept(&ticket, identity.clone()));
    assert!(
        provider
            .restore(&candidate, &root, &cancel, &mut |_| {})
            .unwrap()
            .is_none()
    );
    let draft = provider.prepare_cache(&acquired, &root, &cancel).unwrap();
    draft
        .publish(&gate.permit(&ticket, &identity).unwrap())
        .unwrap();
    drop(acquired);
    // New provider instance has neither the prior download nor a network endpoint.
    let restarted = Provider::new("owner/guild", fixture.keys.clone()).unwrap();
    let restored = restarted
        .restore(&candidate, &root, &cancel, &mut |_| {})
        .unwrap()
        .unwrap();
    assert_eq!(
        guild_session::update::bundle::info(restored.bundle()).unwrap(),
        fixture.manifest.build
    );
    // Extracted files are disposable; a cache reload must reconstruct authenticated bytes.
    std::fs::write(
        restored.bundle().join("Contents/Resources/commit"),
        b"tampered",
    )
    .unwrap();
    drop(restored);
    assert!(
        restarted
            .restore(&candidate, &root, &cancel, &mut |_| {})
            .unwrap()
            .is_some()
    );
    let acquired = restarted
        .restore(&candidate, &root, &cancel, &mut |_| {})
        .unwrap()
        .unwrap();
    let draft = restarted.prepare_cache(&acquired, &root, &cancel).unwrap();
    let permit = gate.permit(&ticket, &identity).unwrap();
    gate.change(Preferences::default());
    assert_eq!(draft.publish(&permit), Err(Error::Cancelled));
    let mut changed = candidate.clone();
    changed.archive_id += 1;
    assert!(
        restarted
            .restore(&changed, &root, &cancel, &mut |_| {})
            .unwrap()
            .is_none()
    );
    let (manifest, manifest_bytes, manifest_signature, _) =
        super::fixture("nightly-2026-10-18", Channel::Nightly);
    let nightly = Candidate {
        release_id: 2,
        archive_id: 12,
        signature_id: 13,
        manifest,
        manifest_bytes,
        manifest_signature,
    };
    assert!(
        restarted
            .restore(&nightly, &root, &cancel, &mut |_| {})
            .unwrap()
            .is_none()
    );
    changed = candidate.clone();
    changed.manifest.build.channel = Some(Channel::Nightly);
    assert!(
        restarted
            .restore(&changed, &root, &cancel, &mut |_| {})
            .is_err()
    );
    // Damage the persisted archive rather than the owned extraction copy.
    fn archive_under(path: &std::path::Path) -> Option<std::path::PathBuf> {
        for entry in std::fs::read_dir(path).ok()? {
            let entry = entry.ok()?;
            if entry.file_name() == "archive.zip" {
                return Some(entry.path());
            }
            if entry.file_type().ok()?.is_dir()
                && !entry.file_name().to_string_lossy().starts_with('.')
                && let Some(found) = archive_under(&entry.path())
            {
                return Some(found);
            }
        }
        None
    }
    let archive = archive_under(&root).unwrap();
    std::fs::write(&archive, b"truncated").unwrap();
    assert_eq!(
        restarted
            .restore(&candidate, &root, &cancel, &mut |_| {})
            .unwrap_err(),
        Error::Verification
    );
    // A corrupt hint must not permanently block an explicitly retried download.
    let receipt = archive
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("ready.json");
    std::fs::write(receipt, b"interrupted JSON").unwrap();
    let draft = restarted.prepare_cache(&acquired, &root, &cancel).unwrap();
    let selected = gate.change(preferences.clone());
    gate.acknowledge(selected, &preferences);
    let retry = gate.begin().unwrap();
    assert!(gate.accept(&retry, identity.clone()));
    draft
        .publish(&gate.permit(&retry, &identity).unwrap())
        .unwrap();
    assert!(
        restarted
            .restore(&candidate, &root, &cancel, &mut |_| {})
            .unwrap()
            .is_some()
    );
}

#[test]
fn local_and_remote_use_the_same_final_copy_and_reject_a_replaced_local_artifact() {
    use crate::update::preparation::{Candidate as PreparedCandidate, Failure};
    use guild_session::update::{Preferences, Source, bundle};
    let fixture = signed_payload_fixture(false);
    let (provider, server) = serve_payload(&fixture, fixture.payload.clone());
    let cancel = AtomicBool::new(false);
    let candidate = provider
        .discover(Channel::Stable, &cancel)
        .unwrap()
        .candidates
        .remove(0);
    let acquired = provider
        .download(
            &candidate,
            &fixture.root.path().join("download"),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
    server.join().unwrap();
    let stage = fixture.root.path().join("готовые сборки");
    bundle::stage(acquired.bundle(), &stage, &cancel).unwrap();
    let preferences = Preferences {
        source: Source::Local,
        local_directory: Some(stage.clone()),
        ..Preferences::default()
    };
    let local = PreparedCandidate::local(&preferences, Path::new("/unused-default-stage"), &cancel)
        .unwrap();
    let remote_source = acquired.bundle().to_path_buf();
    let remote = PreparedCandidate::remote(acquired).unwrap();
    let work = fixture.root.path().join("preparation");
    let local_copy = local.prepare(&work, &cancel).unwrap();
    let remote_copy = remote.prepare(&work, &cancel).unwrap();
    assert_eq!(local_copy.info(), Some(&fixture.manifest.build));
    assert_eq!(remote_copy.info(), local_copy.info());
    assert_ne!(local_copy.bundle(), remote_copy.bundle());
    assert_eq!(local_copy.build().commit, fixture.manifest.build.commit);
    let other = signed_payload_fixture(false);
    bundle::stage(&other.root.path().join("Guild.app"), &stage, &cancel).unwrap();
    assert!(matches!(
        local.prepare(&work, &cancel),
        Err(Failure::Changed)
    ));
    let copy = local_copy.bundle().to_path_buf();
    drop(local_copy);
    assert!(!copy.exists());
    // Successfully prepared bytes remain independent of later source replacement.
    bundle::validate(remote_copy.bundle(), Some(&fixture.manifest.build), &cancel).unwrap();
    // A valid new ad-hoc seal cannot substitute bytes after authenticated extraction,
    // even when commit/build_id and the publisher-signed manifest are unchanged.
    std::fs::write(
        remote_source.join("Contents/Resources/substituted"),
        b"different payload",
    )
    .unwrap();
    let out = std::process::Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&remote_source)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(matches!(
        remote.prepare(&work, &cancel),
        Err(Failure::Changed)
    ));
}
#[test]
fn untagged_local_requires_completion_and_legacy_nightly_is_an_explicit_distinct_artifact() {
    use crate::update::preparation::{Candidate as PreparedCandidate, Failure};
    use guild_session::update::{Preferences, Source, bundle};
    use std::process::Command;
    let fixture = signed_payload_fixture(false);
    let app = fixture.root.path().join("Guild.app");
    let resources = app.join("Contents/Resources");
    let stage = fixture.root.path();
    let preferences = Preferences {
        source: Source::Local,
        local_directory: Some(stage.to_path_buf()),
        ..Preferences::default()
    };
    let cancel = AtomicBool::new(false);
    assert!(matches!(
        PreparedCandidate::local(&preferences, Path::new("/unused"), &cancel),
        Err(Failure::Invalid)
    ));
    // Legacy compatibility has no invented embedded UUID or timestamp-based order.
    std::fs::remove_file(resources.join("build-info.json")).unwrap();
    std::fs::write(resources.join("tag"), b"nightly-2026-10-18\n").unwrap();
    let out = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(out.status.success());
    let legacy = PreparedCandidate::local(&preferences, Path::new("/unused"), &cancel).unwrap();
    let prepared = legacy
        .prepare(&fixture.root.path().join("legacy-copy"), &cancel)
        .unwrap();
    assert!(prepared.info().is_none());
    assert_eq!(prepared.build().version(), Some("nightly-2026-10-18"));
    // New untagged development builds still require the producer's ready marker.
    let mut info = update::BuildInfo::local(fixture.manifest.build.commit.clone());
    info.built = fixture.manifest.build.built;
    std::fs::write(
        resources.join("build-info.json"),
        serde_json::to_vec(&info).unwrap(),
    )
    .unwrap();
    std::fs::remove_file(resources.join("tag")).unwrap();
    let out = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(PreparedCandidate::local(&preferences, Path::new("/unused"), &cancel).is_err());
    let completed = fixture.root.path().join("completed");
    bundle::stage(&app, &completed, &cancel).unwrap();
    let preferences = Preferences {
        local_directory: Some(completed),
        ..preferences
    };
    let dev = PreparedCandidate::local(&preferences, Path::new("/unused"), &cancel).unwrap();
    let prepared = dev
        .prepare(&fixture.root.path().join("dev-copy"), &cancel)
        .unwrap();
    assert_eq!(prepared.info(), Some(&info));
    assert!(prepared.build().version().is_none());
    cancel.store(true, Ordering::Release);
    assert!(matches!(
        dev.prepare(&fixture.root.path().join("cancelled"), &cancel),
        Err(Failure::Cancelled)
    ));
}

#[test]
fn local_completion_marker_does_not_bypass_native_compatibility_or_seal_validation() {
    use crate::update::preparation::{Candidate as PreparedCandidate, Failure};
    use guild_session::update::{Preferences, Source, bundle};
    use std::process::Command;
    for invalid in [
        "bundle-id",
        "minimum-os",
        "cli-architecture",
        "seal",
        "root-symlink",
    ] {
        let fixture = signed_payload_fixture(false);
        let cancel = AtomicBool::new(false);
        let stage = fixture.root.path().join("stage");
        bundle::stage(&fixture.root.path().join("Guild.app"), &stage, &cancel).unwrap();
        let app = stage.join("Guild.app");
        match invalid {
            "bundle-id" => {
                assert!(
                    Command::new("/usr/libexec/PlistBuddy")
                        .args(["-c", "Set :CFBundleIdentifier dev.other.app"])
                        .arg(app.join("Contents/Info.plist"))
                        .status()
                        .unwrap()
                        .success()
                );
            }
            "minimum-os" => {
                assert!(
                    Command::new("/usr/libexec/PlistBuddy")
                        .args(["-c", "Set :LSMinimumSystemVersion 99.0"])
                        .arg(app.join("Contents/Info.plist"))
                        .status()
                        .unwrap()
                        .success()
                );
                let mut info = fixture.manifest.build.clone();
                info.min_macos = "99.0".into();
                std::fs::write(
                    app.join("Contents/Resources/build-info.json"),
                    serde_json::to_vec(&info).unwrap(),
                )
                .unwrap();
            }
            "cli-architecture" => {
                let other = signed_payload_fixture(true);
                std::fs::copy(
                    other.root.path().join("Guild.app/Contents/MacOS/guild"),
                    app.join("Contents/MacOS/guild"),
                )
                .unwrap();
            }
            "seal" => {
                std::fs::write(app.join("Contents/Resources/unsigned"), b"tampered").unwrap();
            }
            "root-symlink" => {
                let real = stage.join("elsewhere.app");
                std::fs::rename(&app, &real).unwrap();
                std::os::unix::fs::symlink(real, &app).unwrap();
            }
            _ => unreachable!(),
        }
        if !matches!(invalid, "seal" | "root-symlink") {
            let out = Command::new("/usr/bin/codesign")
                .args(["--force", "--deep", "--sign", "-"])
                .arg(&app)
                .output()
                .unwrap();
            assert!(out.status.success());
        }
        let preferences = Preferences {
            source: Source::Local,
            local_directory: Some(stage),
            ..Preferences::default()
        };
        assert!(
            matches!(
                PreparedCandidate::local(&preferences, Path::new("/unused"), &cancel),
                Err(Failure::Invalid)
            ),
            "{invalid}"
        );
    }
}

#[test]
fn runtime_owns_local_preparation_and_clears_it_before_shutdown_join() {
    use crate::update::{
        control::Gate,
        runtime::{Config, Event, Request, Runtime},
    };
    use guild_session::update::{Preferences, Source, bundle};
    let fixture = signed_payload_fixture(false);
    let cancel = AtomicBool::new(false);
    let stage = fixture.root.path().join("stage");
    bundle::stage(&fixture.root.path().join("Guild.app"), &stage, &cancel).unwrap();
    let preferences = Preferences {
        source: Source::Local,
        local_directory: Some(stage.clone()),
        ..Preferences::default()
    };
    let mut gate = Gate::pending(preferences.clone(), "owner/guild".into());
    gate.acknowledge(gate.selection(), &preferences);
    let mut runtime = Runtime::spawn(
        Config {
            default_stage: stage,
            work: fixture.root.path().join("work"),
            cache: fixture.root.path().join("cache"),
        },
        None,
    )
    .unwrap();
    let ticket = gate.begin().unwrap();
    runtime.submit(Request::Check(ticket.clone()));
    let event = runtime_final(&runtime);
    let Event::Checked {
        candidates, ready, ..
    } = event
    else {
        panic!("{event:?}")
    };
    let identity = candidates[0].identity.clone();
    assert_eq!(ready, Some(identity.clone()));
    assert!(gate.accept(&ticket, identity.clone()));
    let ticket = gate.begin().unwrap();
    gate.accept(&ticket, identity.clone());
    runtime.submit(Request::Prepare {
        ticket,
        identity,
        root: fixture.root.path().join("final-copy"),
    });
    let event = runtime_final(&runtime);
    let Event::Prepared { bundle, .. } = event else {
        panic!("{event:?}")
    };
    assert!(bundle.is_dir());
    gate.change(Preferences::default());
    runtime.clear();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !runtime.idle() {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        !bundle.exists(),
        "cleanup stays on the worker although UI retains its path snapshot"
    );
    runtime.stop().unwrap().join().unwrap();
    gate.acknowledge(gate.selection(), &Preferences::default());
    assert!(!runtime.submit(Request::Check(gate.begin().unwrap())));
}
fn runtime_final(runtime: &crate::update::runtime::Runtime) -> crate::update::runtime::Event {
    use crate::update::runtime::Event;
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        for event in runtime.drain() {
            if !matches!(event, Event::Progress { .. }) {
                return event;
            }
        }
        assert!(
            std::time::Instant::now() < until,
            "updater worker did not return an event"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn runtime_remote_cache_survives_worker_restart_but_owned_payloads_are_reaped() {
    use crate::update::{
        control::Gate,
        runtime::{Config, Event, Request, Runtime},
    };
    use guild_session::update::{Preferences, Source};
    let fixture = signed_payload_fixture(false);
    let (provider, server) = serve_payload(&fixture, fixture.payload.clone());
    let preferences = Preferences {
        channel: Channel::Stable,
        ..Preferences::default()
    };
    let mut gate = Gate::pending(preferences.clone(), "owner/guild".into());
    gate.acknowledge(gate.selection(), &preferences);
    let config = || Config {
        default_stage: fixture.root.path().join("stage"),
        work: fixture.root.path().join("work"),
        cache: fixture.root.path().join("cache"),
    };
    let mut runtime = Runtime::spawn(config(), Some(provider)).unwrap();
    let checked = gate.begin().unwrap();
    runtime.submit(Request::Check(checked.clone()));
    let event = runtime_final(&runtime);
    let Event::Checked {
        candidates, ready, ..
    } = event
    else {
        panic!("{event:?}")
    };
    assert!(ready.is_none());
    let identity = candidates[0].identity.clone();
    gate.accept(&checked, identity.clone());
    let ticket = gate.begin().unwrap();
    gate.accept(&ticket, identity.clone());
    runtime.submit(Request::Download {
        ticket: ticket.clone(),
        identity: identity.clone(),
    });
    let event = runtime_final(&runtime);
    let Event::Downloaded { candidate, .. } = event else {
        panic!("{event:?}")
    };
    assert_eq!(candidate.identity, identity);
    server.join().unwrap();
    runtime.submit(Request::Cache {
        ticket: ticket.clone(),
        permit: gate.permit(&ticket, &identity).unwrap(),
    });
    let event = runtime_final(&runtime);
    assert!(matches!(event, Event::Cached { .. }), "{event:?}");
    let ticket = gate.begin().unwrap();
    gate.accept(&ticket, identity.clone());
    runtime.submit(Request::Prepare {
        ticket,
        identity: identity.clone(),
        root: fixture.root.path().join("prepared"),
    });
    let event = runtime_final(&runtime);
    let Event::Prepared { bundle, .. } = event else {
        panic!("{event:?}")
    };
    gate.change(Preferences {
        source: Source::Local,
        ..preferences.clone()
    });
    runtime.clear();
    runtime.stop().unwrap().join().unwrap();
    assert!(!bundle.exists());
    assert!(
        std::fs::read_dir(fixture.root.path().join("work"))
            .unwrap()
            .next()
            .is_none()
    );
    // Fresh metadata discovery succeeds, but no archive/signature HTTP request
    // exists on this restarted fixture server: restore must use verified cache bytes.
    let mut summary = release("v1.2.0", false);
    summary["assets"][2]["size"] = serde_json::json!(fixture.manifest.bytes);
    let (origin, _, restarted_server) = super::server(vec![
        serde_json::to_vec(&vec![summary]).unwrap(),
        fixture.metadata.clone(),
        fixture.metadata_sig.clone(),
    ]);
    let mut provider = Provider::new("owner/guild", fixture.keys.clone()).unwrap();
    provider.origin = origin;
    let mut restarted = Runtime::spawn(config(), Some(provider)).unwrap();
    let selected = gate.change(preferences.clone());
    gate.acknowledge(selected, &preferences);
    let ticket = gate.begin().unwrap();
    restarted.submit(Request::Check(ticket));
    let event = runtime_final(&restarted);
    let Event::Checked {
        ready,
        cache_warning,
        ..
    } = event
    else {
        panic!("{event:?}")
    };
    assert_eq!(ready, Some(identity));
    assert!(cache_warning.is_none());
    restarted_server.join().unwrap();
    restarted.stop().unwrap().join().unwrap();
}

#[test]
fn runtime_source_switch_cancels_private_child_and_runs_only_the_latest_local_job() {
    use crate::update::{
        control::Gate,
        runtime::{Config, Event, Request, Runtime},
    };
    use guild_session::update::{Preferences, Source, bundle};
    let fixture = signed_payload_fixture(false);
    let stage = fixture.root.path().join("stage");
    bundle::stage(
        &fixture.root.path().join("Guild.app"),
        &stage,
        &AtomicBool::new(false),
    )
    .unwrap();
    let fake = fake_gh(b"", false);
    std::fs::write(
        fake.path().join("gh"),
        r#"#!/bin/sh
fixture_dir=$(dirname "$0")
printf 'HTTP/2.0 200 OK\r\n\r\n'
touch "$fixture_dir/started"
(sleep 2; touch "$fixture_dir/survived") &
wait
"#,
    )
    .unwrap();
    let (origin, _, server) = server_status(vec![(401, b"private".to_vec())]);
    let mut provider = Provider::new("owner/guild", fixture.keys.clone()).unwrap();
    provider.origin = origin;
    provider.gh_override = Some(fake.path().join("gh"));
    let mut runtime = Runtime::spawn(
        Config {
            default_stage: stage.clone(),
            work: fixture.root.path().join("work"),
            cache: fixture.root.path().join("cache"),
        },
        Some(provider),
    )
    .unwrap();
    let original = Preferences::default();
    let mut gate = Gate::pending(original.clone(), "owner/guild".into());
    gate.acknowledge(gate.selection(), &original);
    let old = gate.begin().unwrap();
    runtime.submit(Request::Check(old.clone()));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !fake.path().join("started").exists() {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let local = Preferences {
        source: Source::Local,
        local_directory: Some(stage),
        ..original
    };
    let selected = gate.change(local.clone());
    runtime.clear();
    gate.acknowledge(selected, &local);
    let latest = gate.begin().unwrap();
    runtime.submit(Request::Check(latest));
    loop {
        let event = runtime_final(&runtime);
        if !gate.owns(event.ticket()) {
            assert!(!gate.owns(&old));
            continue;
        }
        let Event::Checked {
            candidates, ready, ..
        } = event
        else {
            panic!("{event:?}")
        };
        assert_eq!(ready, Some(candidates[0].identity.clone()));
        break;
    }
    runtime.stop().unwrap().join().unwrap();
    server.join().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2100));
    assert!(
        !fake.path().join("survived").exists(),
        "cancelled gh descendants were reaped before the new Local result"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn journal_native_begin_adopts_only_the_exact_validated_payload_and_abort_restores_old() {
    use crate::update::control::{Artifact, Identity, Origin};
    use guild_session::update::{
        bundle,
        transaction::{Phase, Recovery, Store},
    };
    let old = signed_payload_fixture(false);
    let new = signed_payload_fixture(false);
    let destination = old.root.path().join("Guild.app");
    let staged = new.root.path().join("Guild.app");
    let cancel = AtomicBool::new(false);
    let old_seal = bundle::seal(&destination, &cancel).unwrap();
    let next_seal = bundle::seal(&staged, &cancel).unwrap();
    let store = Store::new(old.root.path().join("isolated-state"));
    let identity = Identity {
        artifact: Artifact::Build(new.manifest.build.build_id),
        origin: Origin::Local {
            directory: None,
            seal: next_seal.clone(),
        },
    };
    let receipt = store
        .begin(
            &destination,
            &staged,
            &identity,
            std::process::id(),
            &cancel,
        )
        .unwrap();
    assert_eq!(receipt.phase(), Phase::Prepared);
    assert_eq!(bundle::seal(&destination, &cancel).unwrap(), old_seal);
    assert_eq!(
        bundle::seal(&receipt.prepared_path(), &cancel).unwrap(),
        next_seal
    );
    assert!(!staged.exists());
    assert_eq!(
        store.pending(&destination).unwrap().unwrap().id(),
        receipt.id()
    );
    let restored = store
        .recover(receipt.id(), Recovery::RestorePrevious, &cancel)
        .unwrap();
    assert_eq!(restored.phase(), Phase::Restored);
    assert_eq!(bundle::seal(&destination, &cancel).unwrap(), old_seal);
    assert!(!receipt.prepared_path().exists());
    assert!(store.pending(&destination).unwrap().is_none());
}

#[test]
fn runtime_transfers_prepared_ownership_into_journal_and_source_clear_aborts_it() {
    use crate::update::{
        control::Gate,
        runtime::{Config, Event, Request, Runtime},
    };
    use guild_session::update::{
        Preferences, Source, bundle,
        transaction::{Phase, Store},
    };
    let old = signed_payload_fixture(false);
    let new = signed_payload_fixture(false);
    let cancel = AtomicBool::new(false);
    let stage = new.root.path().join("stage");
    bundle::stage(&new.root.path().join("Guild.app"), &stage, &cancel).unwrap();
    let preferences = Preferences {
        source: Source::Local,
        local_directory: Some(stage.clone()),
        ..Preferences::default()
    };
    let mut gate = Gate::pending(preferences.clone(), "owner/guild".into());
    gate.acknowledge(gate.selection(), &preferences);
    let mut runtime = Runtime::spawn(
        Config {
            default_stage: stage,
            work: new.root.path().join("work"),
            cache: new.root.path().join("cache"),
        },
        None,
    )
    .unwrap();
    let ticket = gate.begin().unwrap();
    runtime.submit(Request::Check(ticket.clone()));
    let event = runtime_final(&runtime);
    let Event::Checked { candidates, .. } = event else {
        panic!("{event:?}")
    };
    let identity = candidates[0].identity.clone();
    gate.accept(&ticket, identity.clone());
    runtime.submit(Request::Prepare {
        ticket: ticket.clone(),
        identity: identity.clone(),
        root: old.root.path().join("preparation"),
    });
    let event = runtime_final(&runtime);
    let Event::Prepared {
        bundle: prepared, ..
    } = event
    else {
        panic!("{event:?}")
    };
    let state = old.root.path().join("state");
    let destination = old.root.path().join("Guild.app");
    let old_seal = bundle::seal(&destination, &cancel).unwrap();
    runtime.submit(Request::Journal {
        ticket: ticket.clone(),
        identity,
        destination: destination.clone(),
        state: state.clone(),
    });
    let event = runtime_final(&runtime);
    let Event::Journaled { receipt, .. } = event else {
        panic!("{event:?}")
    };
    assert_eq!(receipt.phase(), Phase::Prepared);
    assert!(!prepared.exists());
    assert!(receipt.prepared_path().is_dir());
    assert_eq!(bundle::seal(&destination, &cancel).unwrap(), old_seal);
    // A test executable is never eligible to impersonate the running app's helper.
    runtime.submit(Request::Handoff {
        ticket: ticket.clone(),
        identity: receipt.selection().clone(),
    });
    assert!(matches!(
        runtime_final(&runtime),
        Event::Failed {
            error: crate::update::runtime::Failure::Transaction(
                guild_session::update::transaction::Failure::Changed
            ),
            ..
        }
    ));
    let mut other = Runtime::spawn(
        Config {
            default_stage: preferences.local_directory.clone().unwrap(),
            work: new.root.path().join("other-work"),
            cache: new.root.path().join("other-cache"),
        },
        None,
    )
    .unwrap();
    other.submit(Request::Inspect {
        ticket: ticket.clone(),
        state: state.clone(),
        destination: Some(destination.clone()),
    });
    let Event::Inspected {
        receipt: Some(observed),
        ..
    } = runtime_final(&other)
    else {
        panic!("pending journal was not observed");
    };
    assert_eq!(observed.id(), receipt.id());
    other.submit(Request::Check(ticket.clone()));
    assert!(matches!(runtime_final(&other), Event::Checked { .. }));
    other.submit(Request::Prepare {
        ticket: ticket.clone(),
        identity: receipt.selection().clone(),
        root: old.root.path().join("other-preparation"),
    });
    assert!(matches!(runtime_final(&other), Event::Prepared { .. }));
    other.submit(Request::Journal {
        ticket: ticket.clone(),
        identity: receipt.selection().clone(),
        destination: destination.clone(),
        state: old.root.path().join("other-state"),
    });
    assert!(matches!(
        runtime_final(&other),
        Event::Failed {
            error: crate::update::runtime::Failure::Transaction(
                guild_session::update::transaction::Failure::Conflict
            ),
            ..
        }
    ));
    other.stop().unwrap().join().unwrap();
    assert_eq!(
        Store::new(state.clone())
            .pending(&destination)
            .unwrap()
            .unwrap()
            .id(),
        receipt.id(),
        "a losing worker must never adopt or abort the winner's active receipt"
    );
    gate.change(Preferences::default());
    assert!(!gate.owns(&ticket));
    runtime.clear();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !runtime.idle() {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let store = Store::new(state);
    assert_eq!(store.load(receipt.id()).unwrap().phase(), Phase::Aborted);
    assert!(store.pending(&destination).unwrap().is_none());
    assert!(!receipt.prepared_path().exists());
    assert_eq!(bundle::seal(&destination, &cancel).unwrap(), old_seal);
    runtime.stop().unwrap().join().unwrap();
}

#[test]
fn lightweight_local_poll_preserves_confirmation_and_changed_ready_marker_replaces_it() {
    use crate::update::{
        control::Gate,
        runtime::{Config, Event, Request, Runtime},
    };
    use guild_session::update::{Preferences, Source, bundle};
    let first = signed_payload_fixture(false);
    let second = signed_payload_fixture(false);
    let stage = first.root.path().join("stage");
    let cancel = AtomicBool::new(false);
    bundle::stage(&first.root.path().join("Guild.app"), &stage, &cancel).unwrap();
    let preferences = Preferences {
        source: Source::Local,
        local_directory: Some(stage.clone()),
        ..Preferences::default()
    };
    let mut gate = Gate::pending(preferences.clone(), "owner/guild".into());
    gate.acknowledge(gate.selection(), &preferences);
    let mut runtime = Runtime::spawn(
        Config {
            default_stage: stage.clone(),
            work: first.root.path().join("work"),
            cache: first.root.path().join("cache"),
        },
        None,
    )
    .unwrap();
    let ticket = gate.begin().unwrap();
    runtime.submit(Request::Check(ticket.clone()));
    let event = runtime_final(&runtime);
    let Event::Checked { candidates, .. } = event else {
        panic!("{event:?}")
    };
    let previous = candidates[0].identity.clone();
    assert!(gate.accept(&ticket, previous.clone()));
    let permit = gate.permit(&ticket, &previous).unwrap();
    runtime.submit(Request::PollLocal(ticket.clone()));
    assert!(matches!(runtime_final(&runtime), Event::Unchanged { .. }));
    assert!(gate.pins(&ticket, &previous));
    assert!(permit.pins(&previous));
    // Metadata polling is not repeated code validation. Final preparation must
    // still reject changed bytes even when a producer failed to change metadata.
    std::fs::write(
        stage.join("Guild.app/Contents/MacOS/guild-desktop"),
        b"corrupt",
    )
    .unwrap();
    runtime.submit(Request::PollLocal(ticket.clone()));
    assert!(matches!(runtime_final(&runtime), Event::Unchanged { .. }));
    runtime.submit(Request::Prepare {
        ticket: ticket.clone(),
        identity: previous.clone(),
        root: first.root.path().join("final-copy"),
    });
    assert!(matches!(runtime_final(&runtime), Event::Failed { .. }));
    bundle::stage(&second.root.path().join("Guild.app"), &stage, &cancel).unwrap();
    runtime.submit(Request::PollLocal(ticket.clone()));
    let event = runtime_final(&runtime);
    let Event::Checked { candidates, .. } = event else {
        panic!("{event:?}")
    };
    let next = candidates[0].identity.clone();
    assert_ne!(next, previous);
    assert!(gate.accept(&ticket, next));
    assert!(!gate.pins(&ticket, &previous));
    assert!(!permit.pins(&previous));
    gate.change(Preferences::default());
    gate.acknowledge(gate.selection(), &Preferences::default());
    runtime.submit(Request::PollLocal(gate.begin().unwrap()));
    assert!(
        matches!(
            runtime_final(&runtime),
            Event::Failed {
                error: crate::update::runtime::Failure::Unavailable,
                ..
            }
        ),
        "Remote must never poll or fall back to a Local staging directory"
    );
    runtime.stop().unwrap().join().unwrap();
}
