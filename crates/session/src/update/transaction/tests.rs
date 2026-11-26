use super::*;
use crate::update::{Artifact, Origin};
use std::{cell::Cell, fs};
struct Fixture {
    root: tempfile::TempDir,
    destination: PathBuf,
    staged: PathBuf,
    state: PathBuf,
    old: Proof,
    new: Proof,
    identity: Identity,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("Guild.app");
        let staged = root.path().join("staged.app");
        app(&destination, b"old", Uuid::new_v4());
        app(&staged, b"new", Uuid::new_v4());
        let platform = Fake {
            fail_launch: false,
            launches: Cell::new(0),
        };
        let cancel = AtomicBool::new(false);
        let old = platform.proof(&destination, &cancel).unwrap();
        let new = platform.proof(&staged, &cancel).unwrap();
        let identity = Identity {
            artifact: Artifact::Build(new.build_id.unwrap()),
            origin: Origin::Local {
                directory: None,
                seal: new.seal.clone(),
            },
        };
        let state = root.path().join("state");
        Self {
            root,
            destination,
            staged,
            state,
            old,
            new,
            identity,
        }
    }
    fn store(&self) -> Store {
        Store::new(self.state.clone())
    }
    fn begin(&self, platform: &Fake) -> Receipt {
        self.store()
            .begin_with(
                Admission {
                    destination: &self.destination,
                    staged: &self.staged,
                    identity: &self.identity,
                    parent_pid: 123,
                },
                &AtomicBool::new(false),
                platform,
                &mut |_, _| Ok(()),
            )
            .unwrap()
    }
}
fn app(path: &Path, bytes: &[u8], id: Uuid) {
    fs::create_dir_all(path.join("Contents/MacOS")).unwrap();
    fs::write(path.join("Contents/MacOS/guild-desktop"), bytes).unwrap();
    fs::write(path.join("id"), id.to_string()).unwrap();
}
struct Fake {
    fail_launch: bool,
    launches: Cell<usize>,
}
impl Platform for Fake {
    fn proof(&self, path: &Path, cancel: &AtomicBool) -> Result<Proof, Failure> {
        if fs::symlink_metadata(path)
            .map_err(|_| Failure::Changed)?
            .file_type()
            .is_symlink()
        {
            return Err(Failure::Changed);
        }
        let (_, seal) = bundle::hash_file(&path.join("Contents/MacOS/guild-desktop"), cancel)
            .map_err(|_| Failure::Changed)?;
        let id = fs::read_to_string(path.join("id"))
            .map_err(|_| Failure::Changed)?
            .parse()
            .map_err(|_| Failure::Changed)?;
        Ok(Proof {
            build_id: Some(id),
            commit: "1".repeat(40),
            seal,
        })
    }
    fn launch(&self, _receipt: &Receipt, _old: bool, _state: &Path) -> Result<(), Failure> {
        self.launches.set(self.launches.get() + 1);
        if self.fail_launch {
            Err(Failure::Launch)
        } else {
            Ok(())
        }
    }
}
#[test]
fn missing_ack_keeps_previous_app_and_explicit_restore_recovers_it() {
    let fixture = Fixture::new();
    let platform = Fake {
        fail_launch: false,
        launches: Cell::new(0),
    };
    let receipt = fixture.begin(&platform);
    assert_eq!(
        platform
            .proof(&fixture.destination, &AtomicBool::new(false))
            .unwrap(),
        fixture.old
    );
    assert_eq!(receipt.phase(), Phase::Prepared);
    assert!(receipt.prepared_path().is_dir());
    let result = fixture.store().execute_with(
        receipt.id(),
        &AtomicBool::new(false),
        &platform,
        Duration::ZERO,
        &mut |_, _| Ok(()),
    );
    assert!(matches!(result, Err(Failure::MissingAck)));
    assert_eq!(
        platform
            .proof(&fixture.destination, &AtomicBool::new(false))
            .unwrap(),
        fixture.new
    );
    assert_eq!(
        platform
            .proof(&receipt.previous_path(), &AtomicBool::new(false))
            .unwrap(),
        fixture.old
    );
    assert_eq!(
        fixture.store().load(receipt.id()).unwrap().phase(),
        Phase::RecoveryRequired
    );
    let restored = fixture
        .store()
        .recover_with(
            receipt.id(),
            Recovery::RestorePrevious,
            &AtomicBool::new(false),
            &platform,
        )
        .unwrap();
    assert_eq!(restored.phase(), Phase::Restored);
    assert_eq!(
        platform
            .proof(&fixture.destination, &AtomicBool::new(false))
            .unwrap(),
        fixture.old
    );
    assert!(!receipt.previous_path().exists());
}
#[test]
fn unexpected_backup_and_destination_symlink_never_cost_an_unrelated_tree() {
    let fixture = Fixture::new();
    let platform = Fake {
        fail_launch: false,
        launches: Cell::new(0),
    };
    let receipt = fixture.begin(&platform);
    fs::create_dir(receipt.previous_path()).unwrap();
    fs::write(receipt.previous_path().join("sentinel"), b"keep").unwrap();
    assert!(
        fixture
            .store()
            .execute_with(
                receipt.id(),
                &AtomicBool::new(false),
                &platform,
                Duration::ZERO,
                &mut |_, _| Ok(())
            )
            .is_err()
    );
    assert_eq!(
        fs::read(receipt.previous_path().join("sentinel")).unwrap(),
        b"keep"
    );
    assert_eq!(
        platform
            .proof(&fixture.destination, &AtomicBool::new(false))
            .unwrap(),
        fixture.old
    );
    assert!(
        fixture
            .store()
            .recover_with(
                receipt.id(),
                Recovery::RestorePrevious,
                &AtomicBool::new(false),
                &platform
            )
            .is_err()
    );
}
#[test]
fn a_second_state_cannot_start_a_competing_destination_transaction() {
    let fixture = Fixture::new();
    let platform = Fake {
        fail_launch: false,
        launches: Cell::new(0),
    };
    fixture.begin(&platform);
    let other = fixture.root.path().join("other.app");
    app(&other, b"other", Uuid::new_v4());
    let proof = platform.proof(&other, &AtomicBool::new(false)).unwrap();
    let identity = Identity {
        artifact: Artifact::Build(proof.build_id.unwrap()),
        origin: Origin::Local {
            directory: None,
            seal: proof.seal,
        },
    };
    let competing = Store::new(fixture.root.path().join("another-state"));
    assert!(matches!(
        competing.begin_with(
            Admission {
                destination: &fixture.destination,
                staged: &other,
                identity: &identity,
                parent_pid: 456
            },
            &AtomicBool::new(false),
            &platform,
            &mut |_, _| Ok(())
        ),
        Err(Failure::Conflict)
    ));
    assert!(other.exists());
    assert_eq!(
        platform
            .proof(&fixture.destination, &AtomicBool::new(false))
            .unwrap(),
        fixture.old
    );
}

#[test]
fn every_interrupted_move_can_restore_from_actual_files_without_trusting_phase() {
    for boundary in [
        Boundary::ActiveWritten,
        Boundary::PayloadRetained,
        Boundary::Prepared,
        Boundary::OldMoved,
        Boundary::OldRetained,
        Boundary::NewMoved,
        Boundary::NewInPlace,
        Boundary::LaunchRequested,
    ] {
        let fixture = Fixture::new();
        let platform = Fake {
            fail_launch: false,
            launches: Cell::new(0),
        };
        let cancel = AtomicBool::new(false);
        let mut interrupted_id = None;
        let mut hook = |at, receipt: &Receipt| {
            if at == boundary {
                interrupted_id = Some(receipt.id());
                Err(Failure::Interrupted)
            } else {
                Ok(())
            }
        };
        let result = fixture.store().begin_with(
            Admission {
                destination: &fixture.destination,
                staged: &fixture.staged,
                identity: &fixture.identity,
                parent_pid: 123,
            },
            &cancel,
            &platform,
            &mut hook,
        );
        let result = match result {
            Ok(receipt) => fixture.store().execute_with(
                receipt.id(),
                &cancel,
                &platform,
                Duration::ZERO,
                &mut hook,
            ),
            Err(error) => Err(error),
        };
        assert!(matches!(result, Err(Failure::Interrupted)), "{boundary:?}");
        let id = interrupted_id.unwrap();
        assert_eq!(
            fixture
                .store()
                .pending(&fixture.destination)
                .unwrap()
                .unwrap()
                .id(),
            id
        );
        let restored = fixture
            .store()
            .recover_with(id, Recovery::RestorePrevious, &cancel, &platform)
            .unwrap_or_else(|e| panic!("{boundary:?}: {e:?}"));
        assert_eq!(restored.phase(), Phase::Restored);
        assert_eq!(
            platform.proof(&fixture.destination, &cancel).unwrap(),
            fixture.old
        );
        assert!(
            fixture
                .store()
                .pending(&fixture.destination)
                .unwrap()
                .is_none()
        );
        assert!(!restored.prepared_path().exists());
        assert!(!restored.previous_path().exists());
        assert!(!restored.failed_path().exists());
    }
}
#[test]
fn only_exact_loaded_build_nonce_and_executable_acknowledge_the_new_app() {
    let fixture = Fixture::new();
    let platform = Fake {
        fail_launch: false,
        launches: Cell::new(0),
    };
    let cancel = AtomicBool::new(false);
    let receipt = fixture.begin(&platform);
    let mut acknowledged = false;
    let completed = fixture
        .store()
        .execute_with(
            receipt.id(),
            &cancel,
            &platform,
            Duration::ZERO,
            &mut |at, receipt| {
                if at == Boundary::LaunchRequested {
                    let store = fixture.store();
                    let executable = fixture.destination.join("Contents/MacOS/guild-desktop");
                    for (nonce, build, path) in [
                        (
                            Uuid::new_v4(),
                            fixture.new.build_id.unwrap(),
                            executable.clone(),
                        ),
                        (receipt.nonce, Uuid::new_v4(), executable.clone()),
                        (
                            receipt.nonce,
                            fixture.new.build_id.unwrap(),
                            receipt.previous_path().join("Contents/MacOS/guild-desktop"),
                        ),
                    ] {
                        assert_eq!(
                            store.acknowledge_with(
                                receipt.id(),
                                nonce,
                                build,
                                &path,
                                &cancel,
                                &platform
                            ),
                            Err(Failure::InvalidAck)
                        );
                    }
                    assert!(receipt.previous_path().is_dir());
                    store.acknowledge_with(
                        receipt.id(),
                        receipt.nonce,
                        fixture.new.build_id.unwrap(),
                        &executable,
                        &cancel,
                        &platform,
                    )?;
                    acknowledged = true;
                }
                Ok(())
            },
        )
        .unwrap();
    assert!(acknowledged);
    assert_eq!(completed.phase(), Phase::Acknowledged);
    assert!(!receipt.previous_path().exists());
    assert!(
        fixture
            .store()
            .pending(&fixture.destination)
            .unwrap()
            .is_none()
    );
    let installed = fixture.store().installed().unwrap().unwrap();
    assert_eq!(installed.proof, fixture.new);
    assert_eq!(installed.identity, fixture.identity);
}
#[test]
fn a_failed_launch_restores_the_previous_app_and_retains_the_error() {
    let fixture = Fixture::new();
    let platform = Fake {
        fail_launch: true,
        launches: Cell::new(0),
    };
    let receipt = fixture.begin(&platform);
    assert!(matches!(
        fixture.store().execute_with(
            receipt.id(),
            &AtomicBool::new(false),
            &platform,
            Duration::ZERO,
            &mut |_, _| Ok(())
        ),
        Err(Failure::Launch)
    ));
    assert_eq!(
        platform
            .proof(&fixture.destination, &AtomicBool::new(false))
            .unwrap(),
        fixture.old
    );
    assert_eq!(platform.launches.get(), 2);
    assert_eq!(
        fixture.store().load(receipt.id()).unwrap().phase(),
        Phase::Restored
    );
}

#[test]
fn launch_arguments_keep_unicode_state_and_clear_stale_ack_on_rollback() {
    let fixture = Fixture::new();
    let platform = Fake {
        fail_launch: false,
        launches: std::cell::Cell::new(0),
    };
    let receipt = fixture.begin(&platform);
    let state = Path::new("/tmp/Guild state/данные");
    for old in [false, true] {
        let command = super::native::launch_command(&receipt, old, state);
        let args: Vec<_> = command
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"GUILD_STATE_DIR=/tmp/Guild state/данные".to_owned()));
        assert!(
            args.iter()
                .all(|arg| !arg.contains(&receipt.arm_nonce.to_string()))
        );
        assert!(args.contains(&format!(
            "GUILD_UPDATE_NONCE={}",
            if old {
                String::new()
            } else {
                receipt.nonce.to_string()
            }
        )));
        assert_eq!(args.last().unwrap(), &receipt.destination.to_string_lossy());
    }
}

#[test]
fn unreadable_or_corrupt_ack_never_rolls_back_a_possibly_live_new_instance() {
    let fixture = Fixture::new();
    let platform = Fake {
        fail_launch: false,
        launches: Cell::new(0),
    };
    let cancel = AtomicBool::new(false);
    let receipt = fixture.begin(&platform);
    assert!(
        fixture
            .store()
            .execute_with(
                receipt.id(),
                &cancel,
                &platform,
                Duration::ZERO,
                &mut |at, receipt| {
                    if at == Boundary::LaunchRequested {
                        fs::write(
                            fixture
                                .state
                                .join("update/transactions")
                                .join(format!("{}.ack.json", receipt.id())),
                            b"{broken",
                        )
                        .unwrap();
                    }
                    Ok(())
                }
            )
            .is_err()
    );
    assert_eq!(
        platform.proof(&fixture.destination, &cancel).unwrap(),
        fixture.new
    );
    assert_eq!(
        platform.proof(&receipt.previous_path(), &cancel).unwrap(),
        fixture.old
    );
    assert_eq!(
        fixture.store().load(receipt.id()).unwrap().phase(),
        Phase::RecoveryRequired
    );
    assert_eq!(platform.launches.get(), 1);
}

#[test]
fn a_substituted_destination_is_preserved_and_never_followed_during_recovery() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let platform = Fake {
        fail_launch: false,
        launches: Cell::new(0),
    };
    let cancel = AtomicBool::new(false);
    let receipt = fixture.begin(&platform);
    let foreign = fixture.root.path().join("foreign");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("sentinel"), b"keep").unwrap();
    assert!(
        fixture
            .store()
            .execute_with(
                receipt.id(),
                &cancel,
                &platform,
                Duration::ZERO,
                &mut |at, _| {
                    if at == Boundary::OldMoved {
                        symlink(&foreign, &fixture.destination).unwrap();
                    }
                    Ok(())
                }
            )
            .is_err()
    );
    assert_eq!(fs::read(foreign.join("sentinel")).unwrap(), b"keep");
    assert!(
        fs::symlink_metadata(&fixture.destination)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        platform.proof(&receipt.previous_path(), &cancel).unwrap(),
        fixture.old
    );
    assert!(
        fixture
            .store()
            .recover_with(receipt.id(), Recovery::RestorePrevious, &cancel, &platform)
            .is_err()
    );
    assert_eq!(fs::read(foreign.join("sentinel")).unwrap(), b"keep");
}

#[test]
fn source_cancellation_retires_only_a_prepared_transaction_and_keeps_the_live_old_app() {
    let fixture = Fixture::new();
    let platform = Fake {
        fail_launch: false,
        launches: Cell::new(0),
    };
    let receipt = fixture.begin(&platform);
    let cancel = AtomicBool::new(false);
    let aborted = fixture
        .store()
        .abort_prepared_with(receipt.id(), &cancel, &platform)
        .unwrap();
    assert_eq!(aborted.phase(), Phase::Aborted);
    assert_eq!(
        platform.proof(&fixture.destination, &cancel).unwrap(),
        fixture.old
    );
    assert!(!receipt.prepared_path().exists());
    assert!(
        fixture
            .store()
            .pending(&fixture.destination)
            .unwrap()
            .is_none()
    );
    assert_eq!(platform.launches.get(), 0);
    let fixture = Fixture::new();
    let receipt = fixture.begin(&platform);
    let _ = fixture.store().execute_with(
        receipt.id(),
        &cancel,
        &platform,
        Duration::ZERO,
        &mut |at, _| {
            if at == Boundary::OldMoved {
                Err(Failure::Interrupted)
            } else {
                Ok(())
            }
        },
    );
    assert!(
        fixture
            .store()
            .abort_prepared_with(receipt.id(), &cancel, &platform)
            .is_err()
    );
    assert_eq!(
        platform.proof(&receipt.previous_path(), &cancel).unwrap(),
        fixture.old
    );
    assert!(receipt.prepared_path().is_dir());
}

#[test]
fn review_identical_reinstall_recovers_after_launch_failure_or_missing_ack() {
    for fail_launch in [true, false] {
        let mut fixture = Fixture::new();
        fs::write(fixture.staged.join("Contents/MacOS/guild-desktop"), b"old").unwrap();
        fs::write(
            fixture.staged.join("id"),
            fixture.old.build_id.unwrap().to_string(),
        )
        .unwrap();
        fixture.new = fixture.old.clone();
        fixture.identity = Identity {
            artifact: Artifact::Build(fixture.old.build_id.unwrap()),
            origin: Origin::Local {
                directory: None,
                seal: fixture.old.seal.clone(),
            },
        };
        let platform = Fake {
            fail_launch,
            launches: Cell::new(0),
        };
        let cancel = AtomicBool::new(false);
        let receipt = fixture.begin(&platform);
        assert_eq!(receipt.previous, receipt.next);
        assert_eq!(
            fixture
                .store()
                .execute_with(
                    receipt.id(),
                    &cancel,
                    &platform,
                    Duration::ZERO,
                    &mut |_, _| Ok(())
                )
                .unwrap_err(),
            if fail_launch {
                Failure::Launch
            } else {
                Failure::MissingAck
            }
        );
        if !fail_launch {
            fixture
                .store()
                .recover_with(receipt.id(), Recovery::RestorePrevious, &cancel, &platform)
                .unwrap();
        }
        assert_eq!(
            fixture.store().load(receipt.id()).unwrap().phase(),
            Phase::Restored
        );
        assert_eq!(
            platform.proof(&fixture.destination, &cancel).unwrap(),
            fixture.old
        );
        assert!(!receipt.previous_path().exists());
        assert!(
            fixture
                .store()
                .pending(&fixture.destination)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn review_acknowledged_cleanup_failure_keeps_current_and_can_resume() {
    for backup_already_removed in [false, true] {
        let fixture = Fixture::new();
        let platform = Fake {
            fail_launch: false,
            launches: Cell::new(0),
        };
        let cancel = AtomicBool::new(false);
        let receipt = fixture.begin(&platform);
        let error = fixture
            .store()
            .execute_with(
                receipt.id(),
                &cancel,
                &platform,
                Duration::ZERO,
                &mut |at, receipt| {
                    if at == Boundary::LaunchRequested {
                        fixture.store().acknowledge_with(
                            receipt.id(),
                            receipt.nonce,
                            fixture.new.build_id.unwrap(),
                            &fixture.destination.join("Contents/MacOS/guild-desktop"),
                            &cancel,
                            &platform,
                        )?;
                    }
                    if at == Boundary::Acknowledged {
                        // Model a cleanup write failure both before backup removal
                        // and after removal whose parent sync reports failure.
                        if backup_already_removed {
                            fs::remove_dir_all(receipt.previous_path()).unwrap();
                        }
                        return Err(Failure::Storage);
                    }
                    Ok(())
                },
            )
            .unwrap_err();
        assert_eq!(error, Failure::Storage);
        let pending = fixture
            .store()
            .pending(&fixture.destination)
            .unwrap()
            .unwrap();
        assert_eq!(
            pending.phase(),
            Phase::Acknowledged,
            "startup acknowledgement is durable cleanup authority"
        );
        assert_eq!(pending.failure, Some(Failure::Storage));
        let completed = fixture
            .store()
            .recover_with(receipt.id(), Recovery::KeepCurrent, &cancel, &platform)
            .unwrap();
        assert_eq!(completed.phase(), Phase::Acknowledged);
        assert_eq!(
            platform.proof(&fixture.destination, &cancel).unwrap(),
            fixture.new
        );
        assert_eq!(
            fixture.store().installed().unwrap().unwrap().proof,
            fixture.new
        );
        assert!(
            fixture
                .store()
                .pending(&fixture.destination)
                .unwrap()
                .is_none()
        );
    }
}
