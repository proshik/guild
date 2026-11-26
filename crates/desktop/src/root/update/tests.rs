use super::*;
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use guild_session::update::{Channel, Preferences, Source};
#[test]
fn isolated_root_update_preferences() {
    let state = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::update::sources::tests::real_root_update_preferences",
            "--ignored",
            "--test-threads=1",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .env("GUILD_UPDATER_SOURCE_TEST", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
fn settled(root: &Entity<Root>, cx: &mut VisualTestContext) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        root.update(cx, |root, cx| root.poll_update(cx));
        cx.run_until_parked();
        if root.read_with(cx, |root, _| {
            root.update_preferences_ready() && root.update.inspected && !root.update.inspecting
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "source selection never acknowledged"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[gpui::test]
#[ignore = "run through isolated_root_update_preferences"]
fn real_root_update_preferences(cx: &mut TestAppContext) {
    assert_eq!(
        std::env::var("GUILD_UPDATER_SOURCE_TEST").as_deref(),
        Ok("1")
    );
    cx.executor().allow_parking();
    let state = crate::paths::state_dir();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(Default::default(), state.clone(), cx);
            root.isolated_render = true;
            root.enable_updater().unwrap();
            root
        });
        made = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = made.unwrap();
    initial_greeting_keyboard(&root, cx);
    settled(&root, cx);
    root.update(cx, |root, cx| {
        root.poll_update(cx);
        assert!(
            root.update.activated.is_some(),
            "Remote discovery must start as soon as preferences and recovery are ready"
        );
    });
    assert_eq!(
        root.read_with(cx, |root, _| root.update_preferences()),
        Preferences::default()
    );
    root.update(cx, |root, cx| {
        assert!(
            !root.set_update_preferences(
                Preferences {
                    source: Source::Local,
                    local_directory: Some(PathBuf::from("relative-builds")),
                    ..Preferences::default()
                },
                cx
            ),
            "invalid directory syntax must not enter an unretryable preference save"
        );
        assert!(root.update_preferences_ready());
        assert_eq!(root.update_preferences(), Preferences::default());
    });
    root.update(cx, |root, _| {
        let mut installed = guild_session::update::BuildInfo::local("1".repeat(40));
        installed.revision_order = Some(50);
        for (commit, order, same_artifact, offered) in [
            ("2", 51, false, true),
            ("2", 49, false, false),
            ("1", 50, false, false), // A rebuild requires explicit replacement.
            ("1", 50, true, false),
        ] {
            let mut info = installed.clone();
            if !same_artifact {
                info.build_id = uuid::Uuid::new_v4();
            }
            info.commit = commit.repeat(40);
            info.revision_order = Some(order);
            let identity = crate::update::control::Identity {
                artifact: crate::update::control::Artifact::Build(info.build_id),
                origin: crate::update::control::Origin::Remote {
                    repository: update::REPOSITORY.into(),
                    channel: Channel::Nightly,
                    release_id: 1,
                    sha256: "a".repeat(64),
                },
            };
            root.set_running_update_info(Some(installed.clone()));
            let ticket = root
                .update
                .selection
                .as_mut()
                .unwrap()
                .gate_mut()
                .begin()
                .unwrap();
            assert!(root.accept_source_view(
                &ticket,
                crate::update::runtime::View {
                    identity,
                    build: Build::from(&info),
                    info: Some(info),
                    notes: String::new(),
                }
            ));
            assert_eq!(
                root.update.staged.is_some(),
                offered,
                "only a newer authenticated Remote revision belongs in the titlebar"
            );
        }
        root.set_running_update_info(None);
        root.clear_source_candidate();
    });
    // A historical selection must not redefine the whole authenticated discovery.
    let mut installed = guild_session::update::BuildInfo::local("5".repeat(40));
    installed.revision_order = Some(50);
    let views = [51, 50, 49].map(|order| {
        let mut info = installed.clone();
        if order != 50 {
            info.build_id = uuid::Uuid::new_v4();
            info.commit = if order == 51 { "6" } else { "4" }.repeat(40);
        }
        info.revision_order = Some(order);
        crate::update::runtime::View {
            identity: crate::update::control::Identity {
                artifact: crate::update::control::Artifact::Build(info.build_id),
                origin: crate::update::control::Origin::Remote {
                    repository: update::REPOSITORY.into(),
                    channel: Channel::Nightly,
                    release_id: order,
                    sha256: "a".repeat(64),
                },
            },
            build: Build::from(&info),
            info: Some(info),
            notes: String::new(),
        }
    });
    for (newer_available, selected) in [(true, 2), (true, 1), (false, 2), (false, 1)] {
        root.update(cx, |root, cx| {
            root.set_running_update_info(Some(installed.clone()));
            let ticket = root
                .update
                .selection
                .as_mut()
                .unwrap()
                .gate_mut()
                .begin()
                .unwrap();
            assert!(root.accept_source_view(&ticket, views[selected].clone()));
            root.update.candidates = if newer_available {
                views.to_vec()
            } else {
                views[1..].to_vec()
            };
            root.open_settings_at(crate::view::settings::Section::Updates, cx);
        });
        cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(1100.)));
        cx.run_until_parked();
        root.update(cx, |root, cx| {
            root.settings_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("update-details-toggle").is_some(),
            "the actual update status controls must be rendered"
        );
        assert_eq!(
            cx.debug_bounds("update-no-newer-status").is_some(),
            !newer_available && selected == 2,
            "revision 51 must suppress the no-newer status independently of selecting revision 49 or installed 50"
        );
        assert_eq!(
            cx.debug_bounds("update-installed-status").is_some(),
            !newer_available && selected == 1,
            "installed status must not imply discovery is up to date while revision 51 exists"
        );
    }
    // The card discloses notes and source settings independently.
    root.update(cx, |root, cx| {
        let mut view = views[0].clone();
        view.notes = "### Added\n- A real added entry.\n### Fixed\n- A real fixed entry.".into();
        let ticket = root
            .update
            .selection
            .as_mut()
            .unwrap()
            .gate_mut()
            .begin()
            .unwrap();
        assert!(root.accept_source_view(&ticket, view.clone()));
        root.update.candidates = vec![view];
        root.open_settings_at(crate::view::settings::Section::Updates, cx);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("update-install").is_some(),
        "newer release has one acquisition action"
    );
    assert!(cx.debug_bounds("update-check").is_none());
    assert!(cx.debug_bounds("update-cancel").is_none());
    assert!(cx.debug_bounds("update-notes-toggle").is_some());
    assert!(cx.debug_bounds("update-notes-body").is_none());
    assert!(cx.debug_bounds("setting-update-source").is_none());
    let target = cx.debug_bounds("update-notes-toggle").unwrap().center();
    cx.simulate_click(target, gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-notes-body").is_some());
    assert!(cx.debug_bounds("update-notes-content-Added").is_some());
    assert!(cx.debug_bounds("update-notes-content-Fixed").is_none());
    let notes_bounds = cx.debug_bounds("update-notes-body").unwrap();
    let target = cx.debug_bounds("update-notes-Fixed").unwrap().center();
    cx.simulate_click(target, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-notes-content-Fixed").is_some());
    assert_eq!(notes_bounds, cx.debug_bounds("update-notes-body").unwrap());
    assert!(cx.debug_bounds("update-notes-content-Added").is_none());
    assert!(cx.debug_bounds("setting-update-source").is_none());
    let target = cx.debug_bounds("update-details-toggle").unwrap().center();
    cx.simulate_click(target, gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-technical-details").is_some());
    assert!(cx.debug_bounds("update-notes-body").is_some());
    let target = cx.debug_bounds("update-notes-toggle").unwrap().center();
    cx.simulate_click(target, gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-notes-body").is_none());
    assert!(cx.debug_bounds("update-technical-details").is_some());
    let target = cx.debug_bounds("update-details-toggle").unwrap().center();
    cx.simulate_click(target, gpui::Modifiers::default());
    cx.run_until_parked();
    // A selected category absent in another release must not hide its real notes.
    root.update(cx, |root, cx| {
        root.update.candidates[0].notes = "### Changed\n- A different real entry.".into();
        cx.notify();
    });
    let target = cx.debug_bounds("update-notes-toggle").unwrap().center();
    cx.simulate_click(target, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-notes-content-Changed").is_some());
    assert!(cx.debug_bounds("update-notes-Fixed").is_none());
    // Native disclosure activation follows the same path with Space.
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let key = gpui::Keystroke::parse("space").unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: key.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke: key });
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-notes-body").is_none());
    cohesive_release_notes(&root, cx);
    // Untagged development builds retain metadata but have no revision order.
    let mut dev = installed.clone();
    dev.revision_order = None;
    let mut remote = views[0].clone();
    let remote_info = remote.info.as_mut().unwrap();
    remote_info.tag = Some("nightly-2026-10-27".into());
    remote_info.channel = Some(Channel::Nightly);
    remote_info.provenance = guild_session::update::Provenance::GithubActions;
    remote_info.ci = Some(guild_session::update::CiIdentity {
        repository: update::REPOSITORY.into(),
        run_id: 1,
        attempt: 1,
    });
    remote_info.validate().unwrap();
    dev.validate().unwrap();
    remote.build = Build::from(&*remote_info);
    let mut exact_dev = views[1].clone();
    exact_dev.info = Some(dev.clone());
    exact_dev.build = Build::from(&dev);
    for selected in [remote.clone(), exact_dev.clone()] {
        root.update(cx, |root, cx| {
            root.set_running_update_info(Some(dev.clone()));
            let ticket = root
                .update
                .selection
                .as_mut()
                .unwrap()
                .gate_mut()
                .begin()
                .unwrap();
            assert!(root.accept_source_view(&ticket, selected.clone()));
            root.update.candidates = vec![remote.clone(), exact_dev.clone()];
            root.open_settings_at(crate::view::settings::Section::Updates, cx);
        });
        cx.run_until_parked();
        root.update(cx, |root, cx| {
            root.settings_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("update-details-toggle").is_some());
        assert!(
            cx.debug_bounds("update-no-newer-status").is_none(),
            "an unordered dev build cannot establish that no newer Remote revision exists"
        );
        assert!(
            cx.debug_bounds("update-installed-status").is_none(),
            "selecting the exact installed dev build cannot establish global currency among incomparable commits"
        );
    }
    root.update(cx, |root, _| {
        root.set_running_update_info(None);
        root.clear_source_candidate();
    });
    let native = crate::update::remote::tests::native_bundle_fixture();
    let native_app = native.path().join("Guild.app");
    std::fs::write(
        native_app.join("Contents/Resources/CHANGELOG.md"),
        "## [v1.2.0] - 2026-10-18\n### Added\n- Local release notes.\n",
    )
    .unwrap();
    guild_session::update::bundle::tool(
        "/usr/bin/codesign",
        &[
            "--force".as_ref(),
            "--deep".as_ref(),
            "--sign".as_ref(),
            "-".as_ref(),
            native_app.as_os_str(),
        ],
        &AtomicBool::new(false),
    )
    .unwrap();
    guild_session::update::bundle::stage(
        &native.path().join("Guild.app"),
        &state.join("completed local builds"),
        &AtomicBool::new(false),
    )
    .unwrap();
    let desired = Preferences {
        source: Source::Local,
        channel: Channel::Stable,
        local_directory: Some(state.join("completed local builds")),
    };
    root.update(cx, |root, cx| {
        assert!(root.set_update_preferences(desired.clone(), cx));
        assert!(
            !root.update_preferences_ready(),
            "unsaved choice must not activate"
        );
    });
    settled(&root, cx);
    assert_eq!(
        root.read_with(cx, |root, _| root.update_preferences()),
        desired
    );
    // A cold real frame can delay the UI beyond the Local poll interval. The
    // already-completed check reply must be consumed before another poll queues.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.read_with(cx, |root, _| root.update.runtime.as_ref().unwrap().idle()) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    root.update(cx, |root, cx| {
        root.update.looked = Some(Instant::now() - Duration::from_secs(4));
        root.poll_update(cx);
        assert!(
            root.update_download_ready(),
            "a due Local poll must not erase a completed candidate reply"
        );
    });

    root.update(cx, |root, _| {
        assert_eq!(
            root.update.staged_notes,
            vec![(
                guild_session::changelog::Group::Added,
                vec!["Local release notes.".to_owned()]
            )]
        );
    });
    // Real worker failures and successful background replies share the same ticket.
    let directory = desired.local_directory.as_ref().unwrap();
    let parked = state.join("parked completed builds");
    std::fs::rename(directory, &parked).unwrap();
    root.update(cx, |root, _| {
        root.update
            .runtime
            .as_ref()
            .unwrap()
            .submit(crate::update::runtime::Request::PollLocal(
                root.update.ticket.clone().unwrap(),
            ));
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.read_with(cx, |root, _| root.update_failure().is_some()) {
        root.update(cx, |root, cx| root.poll_update(cx));
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::rename(&parked, directory).unwrap();
    root.update(cx, |root, _| {
        root.update
            .runtime
            .as_ref()
            .unwrap()
            .submit(crate::update::runtime::Request::PollLocal(
                root.update.ticket.clone().unwrap(),
            ));
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.read_with(cx, |root, _| root.update_download_ready()) {
        root.update(cx, |root, cx| root.poll_update(cx));
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        root.read_with(cx, |root, _| root.update_failure().is_none()),
        "successful Local poll must clear the preceding error"
    );
    // A terminal receipt can be sampled before its active pointer is retired.
    // Re-read it after helper cleanup instead of permanently blocking the GUI.
    let transient = crate::update::remote::tests::native_bundle_fixture();
    let input = transient.path().join("Guild.app");
    let info = guild_session::update::bundle::info(&input).unwrap();
    let identity = crate::update::control::Identity {
        artifact: crate::update::control::Artifact::Build(info.build_id),
        origin: crate::update::control::Origin::Local {
            directory: None,
            seal: guild_session::update::bundle::seal(&input, &AtomicBool::new(false)).unwrap(),
        },
    };
    let journal = guild_session::update::transaction::Store::new(state.clone());
    let receipt = journal
        .begin(
            &native_app,
            &input,
            &identity,
            std::process::id(),
            &AtomicBool::new(false),
        )
        .unwrap();
    let terminal = journal
        .abort_prepared(receipt.id(), &AtomicBool::new(false))
        .unwrap();
    root.update(cx, |root, cx| {
        root.update.bundle = Some(native_app.clone());
        root.update.recovery = Some(Box::new(terminal));
        root.update.recovery_checked = Some(Instant::now() - Duration::from_secs(4));
        root.poll_update(cx);
        assert!(
            root.update.inspecting,
            "terminal pending receipt must be re-read after helper cleanup"
        );
        assert!(
            !root.update_download_ready(),
            "inspection revokes older candidate authority"
        );
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        root.update(cx, |root, cx| root.poll_update(cx));
        if root.read_with(cx, |root, _| {
            root.update_recovery().is_none() && root.update_download_ready()
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cleaned terminal receipt stranded the updater"
        );
        std::thread::sleep(Duration::from_millis(5));
    }

    // Use the actual common Settings menus, rather than calling a preference setter.
    root.update(cx, |root, cx| {
        root.open_settings_at(crate::view::settings::Section::Updates, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(1100.)));
    cx.run_until_parked();
    root.update(cx, |root, cx| {
        root.settings_scroll.scroll_to_bottom();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-technical-details").is_none());
    let details = cx
        .debug_bounds("update-details-toggle")
        .expect("technical details are collapsed behind an explicit control");
    cx.simulate_click(details.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-technical-details").is_some());
    cx.simulate_click(details.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-technical-details").is_none());
    root.update(cx, |root, cx| {
        root.settings_scroll.set_offset(Default::default());
        cx.notify();
    });
    cx.run_until_parked();
    let target = cx.debug_bounds("update-details-toggle").unwrap().center();
    cx.simulate_click(target, Default::default());
    cx.run_until_parked();
    for (control, item, source, channel) in [
        (
            "setting-update-source",
            "menu-item-0",
            Source::Remote,
            Channel::Stable,
        ),
        (
            "setting-update-channel",
            "menu-item-0",
            Source::Remote,
            Channel::Nightly,
        ),
        (
            "setting-update-source",
            "menu-item-1",
            Source::Local,
            Channel::Nightly,
        ),
        (
            "setting-update-channel",
            "menu-item-1",
            Source::Local,
            Channel::Stable,
        ),
    ] {
        let bounds = cx
            .debug_bounds(control)
            .expect("actual updater menu button");
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
        let bounds = cx.debug_bounds(item).expect("actual updater menu item");
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
        settled(&root, cx);
        assert_eq!(
            root.read_with(cx, |root, _| root.update_preferences()),
            Preferences {
                source,
                channel,
                local_directory: desired.local_directory.clone()
            }
        );
        if source == Source::Remote && channel == Channel::Stable {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !root.read_with(cx, |root, _| root.update.runtime.as_ref().unwrap().idle()) {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            root.update(cx, |root, cx| {
                root.update.activated =
                    Some(root.update.selection.as_ref().unwrap().gate().selection());
                root.update.looked = Some(Instant::now());
                root.poll_update(cx); // Drain the preceding completed provider reply.
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                root.update.error = Some(crate::update::runtime::Failure::Remote(
                    crate::update::remote::Error::RateLimited(now + 7200),
                ));
                root.update.looked = Some(Instant::now() - Duration::from_secs(31 * 60));
                root.update.checking = false;
                root.poll_update(cx);
                assert!(
                    !root.update_checking(),
                    "Remote periodic checks must honor the server's Retry-After deadline"
                );
                for error in [
                    crate::update::remote::Error::MissingGh,
                    crate::update::remote::Error::Access,
                ] {
                    root.update.error = Some(crate::update::runtime::Failure::Remote(error));
                    root.poll_update(cx);
                    assert!(
                        !root.update_checking(),
                        "setup failures require explicit Check now"
                    );
                }
                root.update.error = Some(crate::update::runtime::Failure::Remote(
                    crate::update::remote::Error::RateLimited(now - 1),
                ));
                root.update.checking = false;
                root.poll_update(cx);
                assert!(
                    root.update_checking(),
                    "an idle Remote source is checked again after its scheduled interval"
                );
            });
        }
    }
    let bounds = cx
        .debug_bounds("update-directory-field")
        .expect("real shared directory input");
    cx.simulate_click(bounds.center(), Default::default());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("relative invalid path");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |root, _| root.update_preferences()),
        desired,
        "invalid draft cannot change the persisted directory"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |root, _| root.update_preferences()),
        desired
    );

    let project_path = state.join("settings-host-project");
    std::fs::create_dir(&project_path).unwrap();
    root.update(cx, |root, cx| {
        let project = root.desk.add_project(&project_path);
        root.active_project = Some(project);
        root.settings.language = guild_session::Language::Russian;
        root.settings_page = None;
        root.open_settings_at(crate::view::settings::Section::Updates, cx);
        assert!(matches!(
            root.active_body(),
            Some(crate::Body::Settings { .. })
        ));
    });
    cx.simulate_resize(gpui::size(gpui::px(780.), gpui::px(740.)));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("setting-update-source").is_some(),
        "tab host renders the same source control at a narrow width"
    );
    assert!(cx.debug_bounds("setting-update-channel").is_some());
    root.update(cx, |root, _| {
        root.settings.language = guild_session::Language::English
    });

    let mut conflicting = guild_session::update::bundle::info(&native_app).unwrap();
    conflicting.built += 1;
    root.update(cx, |root, cx| {
        root.set_running_update_info(Some(conflicting));
        root.check_source_update(cx);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        root.update(cx, |root, cx| root.poll_update(cx));
        if !root.read_with(cx, |root, _| root.update_checking()) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    root.read_with(cx, |root, _| {
        assert_eq!(
            root.update_failure(),
            Some(&crate::update::runtime::Failure::Preparation(
                crate::update::preparation::Failure::Invalid
            ))
        );
        assert!(!root.update_download_ready());
        assert!(root.selected_update().is_none());
    });
    root.update(cx, |root, _| root.set_running_update_info(None));

    // The real legacy Settings writer has no updater fields to retain.
    guild_session::Store::<guild_session::Settings>::new(state.join("settings.json"))
        .save(&Default::default())
        .unwrap();
    let other = cx.new(|cx| {
        let mut root = Root::diagnostic(Default::default(), state.clone(), cx);
        root.isolated_render = true;
        root.enable_updater().unwrap();
        root
    });
    settled(&other, cx);
    assert_eq!(
        other.read_with(cx, |root, _| root.update_preferences()),
        desired,
        "Root restart must load sidecar, independent of legacy Settings rewrite"
    );
    // Exercise the actual native replacement prompt without a usable payload;
    // only the source/operation ownership of its deferred answer is under test.
    let fake_bundle = state.join("Guild.app");
    std::fs::create_dir_all(&fake_bundle).unwrap();
    std::fs::write(fake_bundle.join("sentinel"), b"old app").unwrap();
    let installed_info = guild_session::update::BuildInfo::local("1".repeat(40));
    let candidate_info = guild_session::update::BuildInfo::local("1".repeat(40));
    root.update(cx, |root, _| {
        root.update.build = Some(Build::from(&installed_info));
        root.set_running_update_info(Some(installed_info.clone()));
        let info = candidate_info.clone();
        let build = Build::from(&info);
        let identity = crate::update::control::Identity {
            artifact: crate::update::control::Artifact::Build(info.build_id),
            origin: crate::update::control::Origin::Local {
                directory: desired.local_directory.clone(),
                seal: "a".repeat(64),
            },
        };
        let ticket = root
            .update
            .selection
            .as_mut()
            .unwrap()
            .gate_mut()
            .begin()
            .unwrap();
        assert!(
            root.update
                .selection
                .as_mut()
                .unwrap()
                .gate_mut()
                .accept(&ticket, identity.clone())
        );
        root.update.bundle = Some(fake_bundle.clone());
        root.update.ticket = Some(ticket);
        root.update.selected = Some(identity.clone());
        root.update.candidates = vec![crate::update::runtime::View {
            identity,
            build: build.clone(),
            info: Some(info),
            notes: String::new(),
        }];
        root.update.staged = Some(build);
        root.update.ready = true;
        root.update.error = None;
    });
    let journal_entries = || {
        std::fs::read_dir(state.join("update/transactions"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let before_confirmation = journal_entries();
    cx.update(|window, cx| root.update(cx, |root, cx| root.apply_update(window, cx)));
    cx.run_until_parked();
    let (_, body) = cx
        .pending_prompt()
        .expect("replacement asks before preparing");
    assert!(body.contains("Local"));
    assert!(body.contains(&installed_info.build_id.to_string()));
    assert!(body.contains(&candidate_info.build_id.to_string()));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!root.read_with(cx, |root, _| root.update.installing));
    assert_eq!(
        journal_entries(),
        before_confirmation,
        "cancelled or stale confirmation cannot create a transaction"
    );
    // A legacy rollback keeps the sidecar but cannot use its Remote settings.
    root.update(cx, |root, _| root.update.candidates[0].info = None);
    cx.update(|window, cx| root.update(cx, |root, cx| root.apply_update(window, cx)));
    cx.run_until_parked();
    let (_, legacy_body) = cx.pending_prompt().expect("legacy replacement asks");
    assert!(
        legacy_body.contains("only Local") && legacy_body.contains("retained"),
        "legacy rollback must explain old updater limitations: {legacy_body}"
    );
    let changed = Preferences {
        channel: Channel::Nightly,
        ..desired.clone()
    };
    root.update(cx, |root, cx| {
        assert!(root.set_update_preferences(changed, cx))
    });
    settled(&root, cx);
    cx.simulate_prompt_answer("Replace and Restart");
    cx.run_until_parked();
    assert!(
        !root.read_with(cx, |root, _| root.update.installing),
        "late confirmation from a previous selection must not submit preparation"
    );
    assert_eq!(
        journal_entries(),
        before_confirmation,
        "cancelled or stale confirmation cannot create a transaction"
    );
    assert_eq!(
        std::fs::read(fake_bundle.join("sentinel")).unwrap(),
        b"old app"
    );
    // Quit must finish an in-flight save and the latest coalesced choice, without
    // allowing an intermediate Remote acknowledgement to activate its provider.
    root.update(cx, |root, cx| {
        assert!(root.set_update_preferences(
            Preferences {
                source: Source::Remote,
                ..desired.clone()
            },
            cx
        ));
        root.poll_update(cx);
        assert!(root.set_update_preferences(desired.clone(), cx));
    });
    let first = root.update(cx, |root, cx| root.finish_update(cx));
    let second = other.update(cx, |root, cx| root.finish_update(cx));
    cx.foreground_executor().block_test(async {
        first.await;
        second.await;
    });
    assert_eq!(
        guild_session::update::PreferenceStore::new(state)
            .load()
            .unwrap(),
        guild_session::update::PreferenceState::Ready(desired),
        "quit drains the latest pending source instead of only its earlier in-flight save"
    );
}

// Exercise production overlays, rather than a second mock notes renderer.
fn cohesive_release_notes(root: &Entity<Root>, cx: &mut VisualTestContext) {
    use guild_session::changelog::Group;
    assert_eq!(
        crate::view::release_notes::parse_body(
            "### Fixed\n- Первая строка.\n  Настоящая вторая строка."
        ),
        vec![(
            Group::Fixed,
            vec!["Первая строка.\n  Настоящая вторая строка.".into()]
        )]
    );
    assert!(crate::view::release_notes::parse_body("Свободный исходный текст.").is_empty());
    let previous_focus = cx.update(|window, cx| window.focused(cx));
    root.update(cx, |root, cx| {
        root.update.candidates[0].notes = format!(
            "### Added\n{}\n### Changed\n- Короткая запись.",
            (0..20)
                .map(|_| "- Длинная запись с кириллицей для настоящей прокрутки. ".repeat(4))
                .collect::<Vec<_>>()
                .join("\n")
        );
        root.update.greeting = Some(crate::update::Greeting {
            version: "nightly-2026-11-01".into(),
            since: Some("nightly-2026-10-29".into()),
            releases: vec![
                crate::update::Section {
                    version: "nightly-2026-11-01".into(),
                    date: Some("2026-11-01".into()),
                    notes: vec![
                        (
                            Group::Added,
                            vec!["Длинная запись о настоящем изменении. ".repeat(12)],
                        ),
                        (Group::Changed, vec!["Изменение нового релиза.".into()]),
                        (Group::Fixed, vec!["Исправление нового релиза.".into()]),
                    ],
                },
                crate::update::Section {
                    version: "nightly-2026-10-31".into(),
                    date: Some("2026-10-31".into()),
                    notes: vec![
                        (
                            Group::Security,
                            vec![
                                "Исправление пропущенного релиза.\nПодлинная вторая строка.".into(),
                            ],
                        ),
                        (
                            Group::Deprecated,
                            vec!["Устаревшая возможность пропущенного релиза.".into()],
                        ),
                        (
                            Group::Removed,
                            vec!["Удалённая возможность пропущенного релиза.".into()],
                        ),
                    ],
                },
            ],
        });
        cx.notify();
    });
    cx.run_until_parked();
    let row = ["whats-new-Added", "whats-new-Changed", "whats-new-Fixed"]
        .map(|id| cx.debug_bounds(id).unwrap());
    for bounds in row {
        assert_eq!(bounds.top(), row[0].top());
        assert!((bounds.size.width - row[0].size.width).abs() <= gpui::px(1.));
    }
    let added = cx.debug_bounds("whats-new-Added").unwrap().center();
    cx.simulate_click(added, Default::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("tab");
    let key = gpui::Keystroke::parse("space").unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: key.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke: key });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("whats-new-release-nightly-2026-10-31")
            .is_some(),
        "native Tab/Space reaches mapped Changed entries in the missed release"
    );
    let before = cx.debug_bounds("whats-new").unwrap();
    let footer = cx.debug_bounds("whats-new-continue").unwrap();
    let fixed = cx.debug_bounds("whats-new-Fixed").unwrap().center();
    cx.simulate_click(fixed, Default::default());
    cx.run_until_parked();
    assert_eq!(before, cx.debug_bounds("whats-new").unwrap());
    assert_eq!(footer, cx.debug_bounds("whats-new-continue").unwrap());
    for selector in [
        "whats-new-release-nightly-2026-11-01",
        "whats-new-release-nightly-2026-10-31",
    ] {
        assert!(cx.debug_bounds(selector).is_some());
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(root.read_with(cx, |root, _| root.update.greeting().is_none()));
    assert_eq!(previous_focus, cx.update(|window, cx| window.focused(cx)));
    // Every real category remains reachable at the largest UI size in a narrow viewport.
    let font = root.read_with(cx, |root, _| root.settings.ui_font_size);
    root.update(cx, |root, cx| {
        root.settings.ui_font_size = guild_session::MAX_UI_FONT_SIZE;
        root.update.greeting = Some(crate::update::Greeting {
            version: "nightly-2026-11-01".into(),
            since: None,
            releases: vec![crate::update::Section {
                version: "nightly-2026-11-01".into(),
                date: Some("2026-11-01".into()),
                notes: Group::ALL
                    .into_iter()
                    .map(|group| (group, vec!["Подлинная запись релиза.".into()]))
                    .collect(),
            }],
        });
        cx.notify();
    });
    cx.simulate_resize(gpui::size(gpui::px(700.), gpui::px(740.)));
    cx.run_until_parked();
    let card = cx.debug_bounds("whats-new").unwrap();
    let viewport = cx.update(|window, _| window.viewport_size());
    assert!(
        card.left() >= gpui::px(0.) && card.right() <= viewport.width,
        "card {card:?}, viewport {viewport:?}"
    );
    assert!(
        card.top() >= gpui::px(0.) && card.bottom() <= viewport.height,
        "card {card:?}, viewport {viewport:?}"
    );
    for selector in ["whats-new-Added", "whats-new-Changed", "whats-new-Fixed"] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(card.contains(&bounds.center()));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(card, cx.debug_bounds("whats-new").unwrap());
    }
    for selector in [
        "whats-new-Deprecated",
        "whats-new-Removed",
        "whats-new-Security",
    ] {
        assert!(cx.debug_bounds(selector).is_none());
    }
    let proceed = cx.debug_bounds("whats-new-continue").unwrap().center();
    cx.simulate_click(proceed, Default::default());
    cx.run_until_parked();
    assert!(root.read_with(cx, |root, _| root.update.greeting().is_none()));
    // Empty greeting content retains a dismissible modal; backdrop uses the same lifecycle.
    root.update(cx, |root, cx| {
        root.settings.ui_font_size = font;
        root.update.greeting = Some(crate::update::Greeting {
            version: "nightly-2026-11-01".into(),
            since: None,
            releases: vec![],
        });
        cx.notify();
    });
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(1100.)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("whats-new-continue").is_some());
    cx.simulate_click(gpui::point(gpui::px(8.), gpui::px(8.)), Default::default());
    cx.run_until_parked();
    assert!(root.read_with(cx, |root, _| root.update.greeting().is_none()));
    // Production Settings preserves unparsed text and hides absent notes.
    let rich = root.read_with(cx, |root, _| root.update.candidates[0].notes.clone());
    root.update(cx, |root, cx| {
        root.update.candidates[0].notes =
            "Свободный исходный текст.\nБез придуманных разделов.".into();
        cx.notify();
    });
    cx.run_until_parked();
    let toggle = cx.debug_bounds("update-notes-toggle").unwrap().center();
    cx.simulate_click(toggle, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-notes-raw").is_some());
    root.update(cx, |root, cx| {
        root.update.candidates[0].notes.clear();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-notes-toggle").is_none());
    root.update(cx, |root, cx| {
        root.update.candidates[0].notes = rich;
        cx.notify();
    });
    cx.run_until_parked();
    let offer = root.read_with(cx, |root, _| root.notes_offer().unwrap());
    let first_open_focus = cx.update(|window, cx| window.focused(cx));
    let trigger = cx.debug_bounds("titlebar-update").unwrap().center();
    cx.simulate_mouse_move(trigger, None, Default::default());
    cx.executor().advance_clock(Duration::from_millis(550));
    cx.run_until_parked();
    let card = cx.debug_bounds("update-notes-popover").unwrap();
    let action = cx.debug_bounds("update-notes-action").unwrap();
    let scroll = cx.debug_bounds("popover-notes-scroll").unwrap();
    cx.simulate_mouse_move(scroll.center(), None, Default::default());
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: scroll.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-220.))),
        touch_phase: gpui::TouchPhase::Moved,
        ..Default::default()
    });
    cx.run_until_parked();
    assert_eq!(action, cx.debug_bounds("update-notes-action").unwrap());
    let changed = cx.debug_bounds("popover-Changed").unwrap().center();
    cx.simulate_mouse_move(changed, None, Default::default());
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(card, cx.debug_bounds("update-notes-popover").unwrap());
    cx.simulate_click(changed, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("popover-content-Changed").is_some());
    assert_eq!(card, cx.debug_bounds("update-notes-popover").unwrap());
    cx.simulate_keystrokes("shift-tab");
    let key = gpui::Keystroke::parse("space").unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: key.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke: key });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("popover-content-Added").is_some(),
        "popover native Tab/Space selects the previous category"
    );
    let inside = cx.debug_bounds("popover-Added").unwrap().center();
    cx.simulate_mouse_move(
        gpui::point(gpui::px(1050.), gpui::px(1000.)),
        None,
        Default::default(),
    );
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("update-notes-popover").is_some(),
        "departure retains the grace period"
    );
    cx.simulate_mouse_move(inside, None, Default::default());
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("update-notes-popover").is_some(),
        "real reentry cancels the pending departure"
    );
    for (index, x) in [1050., 1040., 1030., 1020.].into_iter().enumerate() {
        cx.simulate_mouse_move(
            gpui::point(gpui::px(x), gpui::px(1000.)),
            None,
            Default::default(),
        );
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        if index < 2 {
            assert!(
                cx.debug_bounds("update-notes-popover").is_some(),
                "outside motion must not bypass the grace period"
            );
        }
    }
    assert!(
        cx.debug_bounds("update-notes-popover").is_none(),
        "continuous outside motion must not restart the original 300ms deadline"
    );
    // A new opening of the same offer follows the current anchor and return focus.
    let folded = root.read_with(cx, |root, _| root.settings.column_folded);
    root.update_in(cx, |root, window, cx| {
        root.settings.column_folded = !folded;
        if first_open_focus.as_ref() != Some(&root.settings_search_focus) {
            root.settings_search_focus.focus(window, cx);
        }
        cx.notify();
    });
    cx.run_until_parked();
    if cx.update(|window, cx| window.focused(cx)) == first_open_focus {
        let toggle = cx.debug_bounds("update-notes-toggle").unwrap().center();
        for _ in 0..2 {
            cx.simulate_click(toggle, Default::default());
            cx.run_until_parked();
        }
    }
    let return_focus = cx.update(|window, cx| window.focused(cx));
    assert_ne!(
        first_open_focus, return_focus,
        "reopen must exercise a different real control"
    );
    let moved_trigger = cx.debug_bounds("titlebar-update").unwrap().center();
    assert_ne!(trigger, moved_trigger);
    cx.simulate_mouse_move(moved_trigger, None, Default::default());
    cx.executor().advance_clock(Duration::from_millis(550));
    cx.run_until_parked();
    let changed = cx.debug_bounds("popover-Changed").unwrap().center();
    cx.simulate_mouse_move(changed, None, Default::default());
    cx.run_until_parked();
    cx.simulate_click(changed, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_move(moved_trigger, None, Default::default());
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("update-notes-popover").is_some(),
        "same-offer hover uses the new trigger bounds"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        return_focus,
        cx.update(|window, cx| window.focused(cx)),
        "same-offer Escape returns to the current opening's focus"
    );
    root.update(cx, |root, cx| {
        root.settings.column_folded = folded;
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_mouse_move(
        gpui::point(gpui::px(1050.), gpui::px(1000.)),
        None,
        Default::default(),
    );
    cx.simulate_mouse_move(trigger, None, Default::default());
    cx.executor().advance_clock(Duration::from_millis(550));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("popover-content-Changed").is_some(),
        "reopening retains the category"
    );
    // Notes/selection epoch changes close the displayed surface before any action.
    let original = root.read_with(cx, |root, _| root.update.candidates[0].clone());
    root.update_in(cx, |root, window, cx| {
        root.update.candidates[0].identity.artifact =
            crate::update::control::Artifact::Build(uuid::Uuid::new_v4());
        assert!(!root.apply_notes_offer(&offer, window, cx));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-notes-popover").is_none());
    root.update_in(cx, |root, window, cx| {
        root.update.candidates[0] = original;
        let preferences = root.update_preferences();
        let epoch = root
            .update
            .selection
            .as_mut()
            .unwrap()
            .gate_mut()
            .change(preferences.clone());
        assert!(!root.apply_notes_offer(&offer, window, cx));
        assert!(
            root.update
                .selection
                .as_mut()
                .unwrap()
                .gate_mut()
                .acknowledge(epoch, &preferences)
        );
        cx.notify();
    });
    cx.run_until_parked();
    // Leave the surrounding source-control scenarios in their original collapsed state.
    let toggle = cx.debug_bounds("update-notes-toggle").unwrap().center();
    cx.simulate_click(toggle, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-notes-body").is_none());
}

#[test]
fn six_source_categories_have_three_lossless_display_buckets() {
    use crate::view::release_notes::{display_notes, selected};
    use guild_session::changelog::Group;
    let source: Vec<_> = Group::ALL
        .into_iter()
        .enumerate()
        .map(|(index, group)| {
            (
                group,
                (0..=index)
                    .map(|n| format!("{} {n}\nContinuation", group.name()))
                    .collect(),
            )
        })
        .collect();
    let untouched = source.clone();
    let grouped = display_notes(&source);
    assert_eq!(
        grouped
            .iter()
            .map(|(group, lines)| (*group, lines.len()))
            .collect::<Vec<_>>(),
        vec![(Group::Added, 1), (Group::Changed, 9), (Group::Fixed, 11)]
    );
    let mut expected: Vec<_> = source.iter().flat_map(|(_, lines)| lines.clone()).collect();
    let mut actual: Vec<_> = grouped
        .iter()
        .flat_map(|(_, lines)| lines.clone())
        .collect();
    expected.sort();
    actual.sort();
    assert_eq!(actual, expected);
    assert_eq!(source, untouched);
    assert_eq!(display_notes(&grouped), grouped);
    assert_eq!(
        selected(&source, Some(Group::Removed)),
        Some(Group::Changed)
    );
    assert_eq!(selected(&source, Some(Group::Security)), Some(Group::Fixed));
    assert!(display_notes(&vec![]).is_empty());
}

// No click establishes a focus ancestry before the startup greeting's first frame.
fn initial_greeting_keyboard(root: &Entity<Root>, cx: &mut VisualTestContext) {
    use guild_session::changelog::Group;
    assert!(root.read_with(cx, |root, _| root.active_body().is_none()));
    for key in ["enter", "space", "tab"] {
        root.update(cx, |root, cx| {
            root.update.greeting = Some(crate::update::Greeting {
                version: "nightly-startup-fixture".into(),
                since: None,
                releases: vec![crate::update::Section {
                    version: "nightly-startup-fixture".into(),
                    date: None,
                    notes: [Group::Added, Group::Changed, Group::Fixed]
                        .into_iter()
                        .map(|group| (group, vec!["Startup fixture entry.".into()]))
                        .collect(),
                }],
            });
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let owner = root.read(cx);
            assert!(
                owner.update.presentation.holds_focus(window, cx),
                "Continue must own the initial greeting focus before any click"
            );
        });
        if key == "tab" {
            for stroke in [
                "tab",
                "tab",
                "tab",
                "tab",
                "shift-tab",
                "shift-tab",
                "shift-tab",
                "shift-tab",
            ] {
                cx.simulate_keystrokes(stroke);
                cx.run_until_parked();
                cx.update(|window, cx| {
                    let owner = root.read(cx);
                    assert!(
                        owner.update.presentation.holds_focus(window, cx),
                        "Tab must stay in the modal"
                    );
                    assert!(owner.update.greeting().is_some() && owner.active_body().is_none());
                });
            }
        }
        let stroke = gpui::Keystroke::parse(if key == "tab" { "enter" } else { key }).unwrap();
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: stroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(gpui::KeyUpEvent { keystroke: stroke });
        cx.run_until_parked();
        assert!(
            root.read_with(cx, |root, _| root.update.greeting().is_none()
                && root.active_body().is_none()),
            "initial Continue dismisses on {key} without a background action"
        );
    }
}
