//! Presentation metadata must survive persistence without changing legacy file identity.
use guild_session::{OpenTab, Shows};
use serde_json::{Value, json};

#[test]
fn a_markdown_mode_round_trips_beside_the_legacy_file_path() {
    for path in [
        "README.md",
        "flow.mmd",
        "flow.MERMAID",
        "class.puml",
        "class.PLANTUML",
    ] {
        for mode in ["editor", "split", "preview"] {
            let wire = json!({"shows":{"file":path},"markdown_mode":mode});
            let tab: OpenTab = serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(tab.shows, Shows::File(path.into()));
            assert_eq!(serde_json::to_value(&tab).unwrap(), wire);
            let kept = tab.keeping(|_| true).unwrap();
            assert_eq!(
                serde_json::to_value(kept).unwrap(),
                wire,
                "filtering missing files must retain the surviving mode"
            );
        }
    }
}

#[test]
fn old_source_tabs_keep_their_wire_shape() {
    let wire = json!({"shows":{"file":"src/lib.rs"}});
    let tab: OpenTab = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(tab).unwrap(), wire);
}

#[test]
fn modes_follow_panes_when_a_missing_file_is_removed() {
    let mut panes = guild_session::Panes::one(Shows::File("README.md".into()));
    let first = panes.focused();
    let second = panes
        .split(first, guild_session::layout::Axis::Horizontal)
        .unwrap();
    panes.fill(second, Shows::File("gone.md".into())).unwrap();
    let mut wire = serde_json::to_value(OpenTab::from_panes(panes).unwrap()).unwrap();
    wire["markdown_mode"] = json!("editor");
    wire["markdown_modes"] = json!({serde_json::to_value(first).unwrap().as_str().unwrap():"preview",serde_json::to_value(second).unwrap().as_str().unwrap():"editor"});
    let tab: OpenTab = serde_json::from_value(wire).unwrap();
    let kept = tab
        .keeping(|shows| *shows == Shows::File("README.md".into()))
        .unwrap();
    let kept = serde_json::to_value(kept).unwrap();
    assert_eq!(kept["markdown_mode"], Value::String("preview".into()));
    assert!(
        kept.get("markdown_modes").is_none(),
        "a single pane needs no orphaned pane IDs"
    );
}

#[test]
fn suffix_detection_and_default_do_not_change_source_files() {
    use guild_session::markdown::{Mode, is_markdown};
    assert_eq!(Mode::default(), Mode::Editor);
    for path in ["README.md", "readme.MD", "doc.MarkDown", "a.MARKDOWN"] {
        assert!(is_markdown(std::path::Path::new(path)));
    }
    for path in ["md", "readme.txt", "lib.rs", "a.md.rs", ".markdown"] {
        assert!(!is_markdown(std::path::Path::new(path)));
    }
}

#[test]
fn markdown_default_view_survives_settings_storage_and_defaults_for_legacy_files() {
    use guild_session::{Settings, Store};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<Settings>::new(dir.path().join("settings.json"));
    assert_eq!(
        serde_json::to_value(Settings::default()).unwrap()["markdown_default_view"],
        "editor"
    );
    let legacy: Settings = serde_json::from_value(json!({"autosave": false})).unwrap();
    assert_eq!(
        serde_json::to_value(legacy).unwrap()["markdown_default_view"],
        "editor"
    );
    for mode in ["editor", "split", "preview"] {
        let settings: Settings =
            serde_json::from_value(json!({"markdown_default_view":mode})).unwrap();
        store.save(&settings).unwrap();
        guild_session::store::flush();
        let saved = store.load().unwrap();
        assert_eq!(
            serde_json::to_value(saved).unwrap()["markdown_default_view"],
            mode
        );
    }
}

#[test]
fn diagram_files_share_presentation_without_becoming_markdown() {
    use guild_session::markdown::{has_preview, is_markdown};
    for path in ["a.mmd", "a.MERMAID", "a.puml", "a.PLANTUML"] {
        assert!(has_preview(std::path::Path::new(path)));
        assert!(!is_markdown(std::path::Path::new(path)));
    }
    assert!(has_preview(std::path::Path::new("README.md")));
    assert!(!has_preview(std::path::Path::new("diagram.rs")));
}
