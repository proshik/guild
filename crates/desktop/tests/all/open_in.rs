//! The title bar's Open In — M12.10: which applications it can offer,
//! and the rows for the ones installed.

use guild_desktop::open_in::{self, APPS};

/// The builtin sixteen, each with every bundle identifier it ships under —
/// IntelliJ IDEA as Ultimate, Community and EAP.
#[test]
fn builtin_catalog_keeps_reference_apps_and_external_system_apps() {
    let names: Vec<&str> = APPS.iter().map(|app| app.name).collect();
    assert_eq!(
        names,
        [
            "Finder",
            "Ghostty",
            "Zed",
            "Zed Preview",
            "VS Code",
            "VS Code Insiders",
            "iTerm2",
            "Terminal",
            "Xcode",
            "GoLand",
            "IntelliJ IDEA",
            "PyCharm",
            "PyCharm CE",
            "CLion",
            "Rider",
            "RustRover",
            "Android Studio",
            "Sublime Text",
        ]
    );
    let mut ids: Vec<&str> = APPS
        .iter()
        .flat_map(|app| app.ids.iter().copied())
        .collect();
    let all = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), all, "an identifier is one application's");
    assert!(
        APPS.iter()
            .find(|app| app.name == "IntelliJ IDEA")
            .unwrap()
            .ids
            .contains(&"com.jetbrains.intellij-EAP")
    );
}

/// The installed ones, in the table's order, each by the first of its
/// identifiers that is installed.
#[test]
fn the_rows_are_the_installed_ones_in_the_table_s_order() {
    let installed = [
        "com.jetbrains.intellij-EAP",
        "dev.zed.Zed",
        "com.mitchellh.ghostty",
        "org.example.not-in-the-table",
    ];

    let rows = open_in::rows(|id| installed.contains(&id));

    let found: Vec<(&str, &str)> = rows.iter().map(|row| (row.name, row.id)).collect();
    assert_eq!(
        found,
        [
            ("Ghostty", "com.mitchellh.ghostty"),
            ("Zed", "dev.zed.Zed"),
            ("IntelliJ IDEA", "com.jetbrains.intellij-EAP"),
        ]
    );
    assert!(open_in::rows(|_| false).is_empty());
}

/// Launch Services finds an application by its identifier, and its icon
/// comes back as a PNG near the size asked for — Finder, which every Mac
/// has. Not the 1024-pixel representation: a menu row draws 16.
#[test]
fn an_application_and_its_icon_are_found_by_identifier() {
    let finder = guild_desktop::system::application("com.apple.finder").expect("Finder");
    assert!(finder.ends_with("Finder.app"), "{}", finder.display());
    assert_eq!(
        guild_desktop::system::application("org.example.not-installed"),
        None
    );

    let png = guild_desktop::system::icon_png(&finder, 32.0).expect("an icon");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert!(png.len() < 64 * 1024, "{} bytes", png.len());
}

/// The footnote names the folder, not all of its road: the start, an
/// ellipsis and the last two names — seen on 2026-09-25, a deep path
/// under `/private/tmp` made the menu as wide as the screen.
#[test]
fn the_folder_is_shortened_to_its_start_and_its_last_two_names() {
    assert_eq!(
        open_in::shown("~/work/team/acme/project/guild-update-nightly"),
        "~/…/project/guild-update-nightly"
    );
    assert_eq!(
        open_in::shown("/private/tmp/build-501/x/cache/look/project"),
        "/private/…/look/project"
    );
    assert_eq!(open_in::shown("~/work/guild"), "~/work/guild");
    assert_eq!(open_in::shown("/Users/a/b"), "/Users/a/b");
    assert_eq!(open_in::shown("~"), "~");
}

#[test]
fn selected_target_survives_settings_and_missing_apps_fall_back() {
    use guild_session::settings::OpenInTarget;
    let settings = guild_session::Settings {
        open_in_target: OpenInTarget::App("dev.zed.Zed".into()),
        ..Default::default()
    };
    let saved = serde_json::to_string(&settings).unwrap();
    let restored: guild_session::Settings = serde_json::from_str(&saved).unwrap();
    let installed = open_in::rows(|id| id == "dev.zed.Zed");
    assert_eq!(
        open_in::available_target(&restored.open_in_target, &installed),
        settings.open_in_target
    );
    assert_eq!(
        open_in::available_target(&restored.open_in_target, &[]),
        OpenInTarget::Finder
    );
    assert_eq!(
        open_in::available_target(&OpenInTarget::Terminal, &[]),
        OpenInTarget::Finder
    );
    assert_eq!(
        serde_json::from_str::<guild_session::Settings>("{}")
            .unwrap()
            .open_in_target,
        OpenInTarget::Finder
    );
}

#[test]
fn launch_boundary_uses_the_selected_bundle_and_exact_folder_without_shell() {
    use guild_session::settings::OpenInTarget;
    let folder = std::path::Path::new("/tmp/a worktree; $(untouched)");
    for (target, expected) in [
        (
            OpenInTarget::Finder,
            vec![
                "-b".into(),
                "com.apple.finder".into(),
                folder.as_os_str().to_owned(),
            ],
        ),
        (
            OpenInTarget::App("com.mitchellh.ghostty".into()),
            vec![
                "-b".into(),
                "com.mitchellh.ghostty".into(),
                folder.as_os_str().to_owned(),
            ],
        ),
        (
            OpenInTarget::App("dev.zed.Zed".into()),
            vec![
                "-b".into(),
                "dev.zed.Zed".into(),
                folder.as_os_str().to_owned(),
            ],
        ),
    ] {
        let mut called = false;
        open_in::launch_with(&target, folder, |command| {
            called = true;
            assert_eq!(command.get_program(), "open");
            assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
            Ok(())
        })
        .unwrap();
        assert!(called);
    }
    open_in::launch_with(&OpenInTarget::Terminal, folder, |command| {
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![
                std::ffi::OsStr::new("-b"),
                std::ffi::OsStr::new("com.apple.finder"),
                folder.as_os_str()
            ]
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn installed_saved_variant_remains_selected_when_another_edition_is_installed() {
    use guild_session::settings::OpenInTarget;
    let saved = OpenInTarget::App("com.jetbrains.intellij-EAP".into());
    let rows = open_in::rows_for_target(&saved, |id| {
        ["com.jetbrains.intellij", "com.jetbrains.intellij-EAP"].contains(&id)
    });
    assert_eq!(
        rows,
        vec![open_in::Row {
            name: "IntelliJ IDEA",
            id: "com.jetbrains.intellij-EAP"
        }]
    );
    assert_eq!(open_in::available_target(&saved, &rows), saved);
}

#[test]
fn installed_ghostty_is_discovered_with_its_real_icon() {
    let installed = std::path::Path::new("/Applications/Ghostty.app");
    if !installed.is_dir() {
        return;
    }
    let found = guild_desktop::system::application("com.mitchellh.ghostty")
        .expect("installed Ghostty must be discoverable");
    eprintln!(
        "Ghostty discovery: {} directory={}",
        found.display(),
        found.is_dir()
    );
    assert!(found.is_dir());
    let png = guild_desktop::system::icon_png(&found, 32.).expect("installed Ghostty icon");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn legacy_terminal_resolves_to_configured_external_terminal_without_duplicates() {
    use guild_session::settings::OpenInTarget;
    let saved: guild_session::Settings =
        serde_json::from_str(r#"{"open_in_target":{"kind":"terminal"}}"#).unwrap();
    let rows = open_in::rows(|id| id == "com.mitchellh.ghostty" || id == "com.apple.Terminal");
    assert_eq!(
        open_in::available_target(&saved.open_in_target, &rows),
        OpenInTarget::App("com.apple.Terminal".into())
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row.id == "com.mitchellh.ghostty")
            .count(),
        1
    );
    assert_eq!(
        open_in::available_target(&saved.open_in_target, &[]),
        OpenInTarget::Finder
    );
}

#[test]
fn configured_apps_persist_order_visibility_and_custom_lifecycle() {
    let mut settings = guild_session::Settings::default();
    let config = &mut settings.open_in_apps;
    let defaults = [
        "com.apple.finder",
        "com.mitchellh.ghostty",
        "com.apple.Terminal",
    ];
    config.set_enabled("com.mitchellh.ghostty", false);
    config.move_item("com.apple.Terminal", false, &defaults);
    let id = config.add_custom();
    config.connect(
        &id,
        "/Applications/Custom.app".into(),
        "org.example.custom".into(),
        "Custom",
    );
    config.rename(&id, "My editor".into());
    let restored: guild_session::Settings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(
        restored.open_in_apps.keys(&defaults),
        vec![
            "com.apple.finder",
            "com.apple.Terminal",
            "com.mitchellh.ghostty",
            id.as_str()
        ]
    );
    assert!(!restored.open_in_apps.enabled("com.mitchellh.ghostty"));
    assert_eq!(restored.open_in_apps.custom[0].name, "My editor");
    settings.open_in_apps.remove_custom(&id);
    assert!(!settings.open_in_apps.keys(&defaults).contains(&id));
    assert!(
        serde_json::from_str::<guild_session::Settings>("{}")
            .unwrap()
            .open_in_apps
            .custom
            .is_empty()
    );
}

#[test]
fn installed_inventory_includes_real_apps_and_system_utilities() {
    let apps = guild_desktop::system::installed_applications();
    assert!(!apps.is_empty());
    let paths: std::collections::HashSet<_> = apps.iter().map(|app| &app.path).collect();
    assert_eq!(paths.len(), apps.len());
    for path in [
        "/Applications/Ghostty.app",
        "/System/Applications/Utilities/Activity Monitor.app",
        "/System/Applications/Utilities/Terminal.app",
    ] {
        if std::path::Path::new(path).is_dir() {
            let app = apps
                .iter()
                .find(|app| app.path == std::fs::canonicalize(path).unwrap())
                .expect("installed application in bounded inventory");
            assert!(!app.bundle_id.is_empty());
            assert!(guild_desktop::system::icon_png(&app.path, 32.).is_some());
        }
    }
}

#[test]
fn custom_application_launch_uses_exact_bundle_path_and_folder() {
    let app = std::path::Path::new("/Applications/My Editor; $(untouched).app");
    let folder = std::path::Path::new("/tmp/a worktree");
    open_in::launch_path_with(app, folder, |command| {
        assert_eq!(command.get_program(), "open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                std::ffi::OsStr::new("-a"),
                app.as_os_str(),
                folder.as_os_str()
            ]
        );
        Ok(())
    })
    .unwrap();
}
