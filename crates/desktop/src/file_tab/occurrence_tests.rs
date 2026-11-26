//! Ownership stays private; lifecycle gestures use the same public FileTab APIs.
use super::*;
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn readonly_source_owns_occurrences_before_and_after_editor_replacement(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.txt");
    std::fs::write(&path, "name name").unwrap();
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let tab = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("library.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
            .read_only(true)
        });
        file = Some(tab.clone());
        gpui_component::Root::new(tab, window, cx)
    });
    let file = file.unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tab = file.read(cx);
        let occurrences = tab
            .occurrences
            .as_ref()
            .expect("read-only source owns highlights");
        assert_eq!(occurrences.read(cx).ranges(cx), vec![0..4, 5..9]);
    });
    // Replace text with a non-text state, then text: this exercises both
    // initial realization and the install_read path that makes another editor.
    std::fs::write(&path, b"\0").unwrap();
    cx.update(|window, cx| file.update(cx, |file, cx| file.reload(window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(file.read(cx).editor().is_none()));
    std::fs::write(&path, "next next").unwrap();
    cx.update(|window, cx| file.update(cx, |file, cx| file.reload(window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tab = file.read(cx);
        let occurrences = tab
            .occurrences
            .as_ref()
            .expect("replacement editor owns highlights");
        assert_eq!(occurrences.read(cx).ranges(cx), vec![0..4, 5..9]);
    });
}

#[gpui::test]
fn supported_large_file_skips_decorations_and_reload_to_small_file_restores_them(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.txt");
    let line = format!("{}\n", "a;".repeat(200));
    std::fs::write(&path, line.repeat(40_000)).unwrap();
    let bytes = std::fs::metadata(&path).unwrap().len();
    assert!(bytes > HIGHLIGHT_LIMIT && bytes < OPEN_LIMIT);
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let tab = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("large.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        file = Some(tab.clone());
        gpui_component::Root::new(tab, window, cx)
    });
    let file = file.unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tab = file.read(cx);
        assert_eq!(tab.state(), &FileState::Text);
        assert!(
            tab.occurrences
                .as_ref()
                .unwrap()
                .read(cx)
                .ranges(cx)
                .is_empty()
        );
    });
    std::fs::write(&path, "name name").unwrap();
    cx.update(|window, cx| file.update(cx, |file, cx| file.reload(window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tab = file.read(cx);
        assert_eq!(tab.editor().unwrap().read(cx).value().as_ref(), "name name");
        assert_eq!(
            tab.occurrences.as_ref().unwrap().read(cx).ranges(cx),
            vec![0..4, 5..9],
            "caret={:?}, selection={}",
            tab.editor().unwrap().read(cx).selected_range(),
            tab.editor().unwrap().read(cx).is_copyable()
        );
    });
}
