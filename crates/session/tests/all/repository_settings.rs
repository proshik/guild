use guild_session::{
    Settings,
    repository_settings::{LspPolicy, RepositorySettings},
};
use std::path::Path;
#[test]
fn old_settings_inherit_enabled_defaults() {
    let settings: Settings = serde_json::from_str("{}").unwrap();
    assert!(settings.lsp_enabled(Path::new("/repo/.git"), "rust"));
    assert!(settings.repository_settings.is_empty());
}
#[test]
fn explicit_policy_wins_global_defaults_and_round_trips() {
    let key = Path::new("/repo/.git");
    let mut settings = Settings::default();
    settings.lsp_defaults.insert("rust".into(), false);
    assert!(!settings.lsp_enabled(key, "rust"));
    settings
        .repository_settings
        .insert(key.into(), RepositorySettings::default());
    settings
        .repository_settings
        .get_mut(key)
        .unwrap()
        .lsp
        .insert("rust".into(), LspPolicy::Enabled);
    assert!(settings.lsp_enabled(key, "rust"));
    settings
        .repository_settings
        .get_mut(key)
        .unwrap()
        .lsp
        .insert("future".into(), LspPolicy::Disabled);
    let copy: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(copy.repository_settings, settings.repository_settings);
    settings.lsp_defaults.insert("rust".into(), true);
    settings
        .repository_settings
        .get_mut(key)
        .unwrap()
        .lsp
        .insert("rust".into(), LspPolicy::Disabled);
    assert!(!settings.lsp_enabled(key, "rust"));
    settings
        .repository_settings
        .get_mut(key)
        .unwrap()
        .lsp
        .insert("rust".into(), LspPolicy::Inherit);
    assert!(settings.lsp_enabled(key, "rust"));
}
