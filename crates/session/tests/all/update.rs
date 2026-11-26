//! Source choices must outlive ordinary legacy Settings snapshots.
use guild_session::update::{Channel, PreferenceState, PreferenceStore, Preferences, Source};
#[test]
fn update_defaults_are_remote_nightly() {
    let root = tempfile::tempdir().unwrap();
    let state = PreferenceStore::new(root.path().into()).load().unwrap();
    let PreferenceState::Ready(p) = state else {
        panic!("new installation needs no recovery")
    };
    assert_eq!(p.source, Source::Remote);
    assert_eq!(p.channel, Channel::Nightly);
}
#[test]
fn source_channel_and_custom_directory_survive_a_legacy_settings_round_trip() {
    let root = tempfile::tempdir().unwrap();
    let store = PreferenceStore::new(root.path().into());
    let expected = Preferences {
        source: Source::Local,
        channel: Channel::Stable,
        local_directory: Some(root.path().join("готовые сборки")),
    };
    store.save(&expected).unwrap();
    // The unchanged legacy typed Settings writer only knows its own fields.
    let legacy =
        guild_session::Store::<guild_session::Settings>::new(root.path().join("settings.json"));
    let settings = guild_session::Settings {
        language: guild_session::Language::Russian,
        ..Default::default()
    };
    legacy.save(&settings).unwrap();
    assert_eq!(
        PreferenceStore::new(root.path().into()).load().unwrap(),
        PreferenceState::Ready(expected)
    );
    assert_eq!(
        legacy.load().unwrap().language,
        guild_session::Language::Russian
    );
}
#[test]
fn future_update_settings_are_preserved_instead_of_reset_to_remote() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("update-preferences.json");
    let bytes = br#"{"schema":99,"source":"future","channel":"stable"}"#;
    std::fs::write(&file, bytes).unwrap();
    assert!(PreferenceStore::new(root.path().into()).load().is_err());
    assert_eq!(std::fs::read(file).unwrap(), bytes);
}

#[test]
fn known_preferences_preserve_future_extension_fields_when_changed() {
    let root = tempfile::tempdir().unwrap();
    let store = PreferenceStore::new(root.path().into());
    store.save(&Preferences::default()).unwrap();
    let path = root.path().join("update-preferences.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["future_display_option"] = serde_json::json!({"enabled":true});
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let next = Preferences {
        channel: Channel::Stable,
        ..Default::default()
    };
    store.save(&next).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        saved["future_display_option"],
        serde_json::json!({"enabled":true})
    );
    assert_eq!(store.load().unwrap(), PreferenceState::Ready(next));
}
