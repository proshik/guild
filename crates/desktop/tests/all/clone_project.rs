//! Clone input and destination preservation at the public boundary.
use guild_desktop::clone_project::{Form, repository_name, validate_destination};
use std::path::Path;

#[test]
fn clone_project_accepts_https_and_both_ssh_spellings() {
    for url in [
        "https://example.test/team/project.git",
        "ssh://git@example.test/team/project.git",
        "git@example.test:team/project.git",
    ] {
        assert_eq!(repository_name(url).unwrap(), "project");
    }
    assert_eq!(
        repository_name("  https://example.test/team/project.git  ").unwrap(),
        "project"
    );
}
#[test]
fn clone_project_refuses_credentials_options_and_invalid_names() {
    for url in [
        "",
        "-x",
        "/tmp/repo",
        "file:///tmp/repo",
        "http://host/repo",
        "https://host",
        "https://host/..",
        "https://host/.git",
        "https://user:secret@host/repo",
        "https://token@host/repo",
        "ssh://git:secret@host/repo",
        "https://host/a b",
        "https://host/repo?token=secret",
        "git@:repo",
        "git@host:..",
    ] {
        assert!(repository_name(url).is_err(), "{url}");
    }
}
#[test]
fn clone_project_manual_destination_survives_url_edits() {
    let mut form = Form::new(Path::new("/tmp"));
    form.set_url("https://host/first.git".into());
    assert_eq!(form.directory, "/tmp/first");
    form.set_directory("/tmp/chosen".into());
    form.set_url("git@host:second.git".into());
    assert_eq!(form.directory, "/tmp/chosen");
}
#[test]
fn clone_project_existing_paths_and_missing_parent_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("existing");
    std::fs::create_dir(&dir).unwrap();
    let file = tmp.path().join("file");
    std::fs::write(&file, "keep").unwrap();
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(tmp.path().join("absent"), &link).unwrap();
    for path in [&dir, &file, &link, &tmp.path().join("absent/child")] {
        assert!(validate_destination(path).is_err());
    }
    assert!(validate_destination(&tmp.path().join("new")).is_ok());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "keep");
}

#[test]
fn clone_project_operation_starts_once_and_published_success_wins() {
    use guild_desktop::clone_project::Phase;
    use guild_desktop::git::clone::Outcome;
    let mut phase = Phase::Editing;
    assert!(phase.start());
    assert!(!phase.start());
    phase.cancel();
    assert!(matches!(phase, Phase::Cancelling));
    assert!(phase.complete(Outcome::Succeeded("/tmp/finished".into())));
    assert!(!phase.start(), "opening retry must not clone again");
    assert!(!phase.complete(Outcome::Cancelled { retained: None }));
    assert!(matches!(phase, Phase::Succeeded(_)));
}
#[test]
fn clone_project_failure_keeps_retry_available() {
    use guild_desktop::clone_project::Phase;
    use guild_desktop::git::clone::Outcome;
    let mut phase = Phase::Editing;
    phase.start();
    assert!(phase.complete(Outcome::Failed {
        message: "network".into(),
        retained: None
    }));
    assert!(phase.start());
    phase.cancel();
    assert!(phase.complete(Outcome::Cancelled { retained: None }));
    assert!(phase.start());
}
#[test]
fn clone_project_field_navigation_wraps_and_manual_edits_preserve_choice() {
    use guild_desktop::clone_project::Field;
    let mut field = Field::Url;
    field = field.step(false);
    assert_eq!(field, Field::Submit);
    assert_eq!(field.step(true), Field::Url);
    let mut form = Form::new(Path::new("/tmp"));
    form.set_url("https://host/one.git".into());
    form.set_directory("/tmp/".into());
    assert_eq!(form.directory, "/tmp/");
    form.set_url("https://host/two.git".into());
    assert_eq!(form.directory, "/tmp/");
}
#[test]
fn clone_project_action_is_named_without_default_shortcut() {
    let action = guild_desktop::actions::keymap::Named::GetFromVersionControl;
    assert_eq!(action.name(), "GetFromVersionControl");
    assert!(guild_desktop::actions::stroke_for(action).is_none());
}

#[test]
fn clone_project_rejects_malformed_authorities_and_blank_directory_names() {
    for url in [
        "https://host:bad/repo",
        "ssh://host:99999/repo",
        "https://[broken]/repo",
        "https://./repo",
    ] {
        assert!(repository_name(url).is_err(), "{url}");
    }
    assert!(validate_destination(Path::new("/tmp/   ")).is_err());
    assert_eq!(repository_name("host:team/repo.git").unwrap(), "repo");
    assert_eq!(
        repository_name("ssh://git@[::1]:2222/team/repo.git").unwrap(),
        "repo"
    );
}

#[test]
fn clone_project_shows_parent_before_a_url_and_after_clearing_it() {
    let mut form = Form::new(Path::new("/Users/me/projects"));
    assert_eq!(form.directory, "/Users/me/projects");
    form.set_url("git@host:team/repo.git".into());
    assert_eq!(form.directory, "/Users/me/projects/repo");
    form.set_url(String::new());
    assert_eq!(form.directory, "/Users/me/projects");
    form.set_directory("/Users/me/custom".into());
    form.set_url("git@host:team/other.git".into());
    assert_eq!(form.directory, "/Users/me/custom");
}

#[test]
fn clone_project_default_parent_prefers_recent_then_active_then_home() {
    use guild_desktop::clone_project::default_parent;
    let recent = Path::new("/last/place/repo");
    let active = Path::new("/current/place/project");
    let home = Path::new("/Users/me");
    assert_eq!(
        default_parent(Some(recent), Some(active), home),
        Path::new("/last/place")
    );
    assert_eq!(
        default_parent(None, Some(active), home),
        Path::new("/current/place")
    );
    assert_eq!(default_parent(None, None, home), home);
}
