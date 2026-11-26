use super::*;

#[gpui::test]
fn loading_revisions_are_not_reported_as_absent(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old\n", "new\n");
    cx.update(|_, cx| {
        tab.update(cx, |t, _| {
            t.base = guild_session::Base::Commit("fixture-sha".into());
            t.old.clear();
            t.working.clear();
            t.fetched.clear();
            t.revisions.clear();
            t.rebuild();
            assert_eq!(
                t.revision_labels(0),
                ["Loading…".to_owned(), "Loading…".to_owned()]
            );
        })
    });
}

#[gpui::test]
fn file_counter_opens_tree_and_escape_restores_focus(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old\n", "new\n");
    cx.run_until_parked();
    let counter = cx
        .debug_bounds("diff-file-counter")
        .expect("clickable file counter");
    cx.simulate_mouse_move(counter.center(), None, Default::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(250));
    cx.run_until_parked();
    assert!(cx.debug_bounds("guild-control-tooltip").is_some());
    cx.simulate_click(counter.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("diff-file-picker").is_some());
    assert!(
        cx.debug_bounds("guild-control-tooltip").is_none(),
        "the trigger hint must not cover its open popup"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("diff-file-picker").is_none());
    cx.update(|window, cx| assert!(tab.read(cx).focus.is_focused(window)));
}

#[gpui::test]
fn toolbar_hints_work_on_disabled_boundaries_and_both_locales(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old\n", "new\n");
    for language in [Language::English, Language::Russian] {
        tab.update(cx, |tab, cx| {
            tab.ui = Ui(language);
            cx.notify();
        });
        cx.run_until_parked();
        for selector in [
            "diff-previous",
            "diff-next",
            "diff-file-previous",
            "diff-file-next",
            "diff-edit",
            "diff-side-by-side",
            "diff-unified",
            "diff-file-counter",
            "diff-inline-mode-0",
        ] {
            let bounds = cx.debug_bounds(selector).unwrap();
            cx.simulate_mouse_move(gpui::point(px(900.), px(600.)), None, Default::default());
            cx.simulate_mouse_move(bounds.center(), None, Default::default());
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(250));
            cx.run_until_parked();
            assert!(
                cx.debug_bounds("guild-control-tooltip").is_some(),
                "actual tooltip missing: {selector}, {language:?}"
            );
        }
    }
}

#[gpui::test]
fn picker_scrolls_the_full_hierarchy_and_selects_with_mouse_and_keyboard(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old\n", "new\n");
    tab.update(cx, |tab, cx| {
        tab.target = None;
        for i in 0..100 {
            let path = PathBuf::from(format!("nested/deep/file-{i:03}.txt"));
            tab.files.push(super::super::super::FileDiff::compute(
                path.clone(),
                Some(b"old\n"),
                Some(b"unique nested file\n"),
            ));
            tab.old.insert(path.clone(), Ok(Some(b"old\n".to_vec())));
            tab.working
                .insert(path, Some(b"unique nested file\n".to_vec()));
        }
        tab.rebuild();
        tab.choose_file(100, cx);
    });
    cx.run_until_parked();
    let counter = cx.debug_bounds("diff-file-counter").unwrap();
    cx.simulate_click(counter.center(), Default::default());
    cx.run_until_parked();
    let last = cx
        .debug_bounds("diff-file-choice-100")
        .expect("current nested file is scrolled into view");
    cx.simulate_click(last.center(), Default::default());
    cx.simulate_keystrokes("cmd-a cmd-c");
    cx.update(|_, cx| {
        assert_eq!(tab.read(cx).current_file(), 100);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("unique nested file\n")
        );
    });
    cx.simulate_click(counter.center(), Default::default());
    let scrollbar = cx
        .debug_bounds("diff-picker-scrollbar")
        .expect("long popup list has a visible scroll track");
    let from = gpui::point(scrollbar.center().x, scrollbar.bottom() - px(15.));
    let to = gpui::point(scrollbar.center().x, scrollbar.top() + px(5.));
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("diff-picker-folder-0").is_some(),
        "scrollbar drag reaches the beginning"
    );
    let popup = cx.debug_bounds("diff-file-picker").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: popup.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(10000.))),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    let folder = cx
        .debug_bounds("diff-picker-folder-0")
        .expect("popup wheel reaches the folder");
    cx.simulate_click(folder.center(), Default::default());
    cx.simulate_keystrokes("enter down enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            tab.read(cx).current_file(),
            1,
            "mouse folder disclosure must own subsequent keyboard selection"
        )
    });
    cx.simulate_click(counter.center(), Default::default());
    // End is the root-level review.txt after the directory's 100 children.
    cx.simulate_keystrokes("end enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tab.read(cx).current_file(), 0));
    assert!(cx.debug_bounds("diff-file-picker").is_none());
    cx.simulate_click(counter.center(), Default::default());
    cx.simulate_keystrokes("home left right down enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tab.read(cx).current_file(), 1));
}

#[gpui::test]
fn rapid_inspection_switch_rejects_prior_same_path_and_partial_pairs(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old original\n", "original\n");
    tab.update(cx, |t, cx| {
        let root = std::path::Path::new("/tmp");
        t.viewer.ratio = 0.61;
        t.toggle_mode(cx);
        t.show_inspection(root, "B.txt".into(), "revision-one".into(), cx);
        t.show_inspection(root, "C.txt".into(), "revision-two".into(), cx);
        t.expected_fetch.borrow_mut().insert("C.txt".into(), 9001);
        t.asks
            .send((
                super::super::super::Which::Old,
                "B.txt".into(),
                Ok(Some(b"STALE B\n".to_vec())),
                Some("old:B.txt".into()),
                9000,
            ))
            .unwrap();
        t.asks
            .send((
                super::super::super::Which::Old,
                "C.txt".into(),
                Ok(Some(b"before C\n".to_vec())),
                Some("111111111111:C.txt".into()),
                9001,
            ))
            .unwrap();
        t.poll(cx);
        assert!(
            t.documents.is_empty(),
            "no old content displayed beneath the new identity"
        );
        assert_eq!(
            t.revision_labels(0),
            ["Loading…".to_owned(), "Loading…".to_owned()]
        );
        t.asks
            .send((
                super::super::super::Which::New,
                "C.txt".into(),
                Ok(Some(b"CURRENT C\n".to_vec())),
                Some("222222222222:C.txt".into()),
                9001,
            ))
            .unwrap();
        t.poll(cx);
    });
    settle(&tab, cx);
    tab.update(cx, |t, cx| {
        let root = std::path::Path::new("/tmp");
        assert_eq!(
            t.documents[&PathBuf::from("C.txt")].sources[1]
                .text
                .as_ref(),
            "CURRENT C\n"
        );
        assert_eq!(
            t.revision_labels(0),
            [
                "111111111111:C.txt".to_owned(),
                "222222222222:C.txt".to_owned()
            ]
        );
        t.show_inspection(root, "B.txt".into(), "revision-one".into(), cx);
        t.show_inspection(root, "C.txt".into(), "revision-three".into(), cx);
        t.expected_fetch.borrow_mut().insert("C.txt".into(), 9002);
        t.asks
            .send((
                super::super::super::Which::New,
                "C.txt".into(),
                Ok(Some(b"STALE SAME PATH\n".to_vec())),
                Some("222222222222:C.txt".into()),
                9001,
            ))
            .unwrap();
        t.poll(cx);
        assert!(t.documents.is_empty());
        assert_eq!(t.files()[0].path, PathBuf::from("C.txt"));
        assert_eq!(t.mode(), DiffView::Unified);
        assert_eq!(t.viewer.ratio, 0.61);
    });
}

#[gpui::test]
fn chrome_geometry_survives_mode_and_loading_at_normal_and_narrow_widths(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old\n", "new\n");
    for width in [1000., 320., 160.] {
        cx.simulate_resize(gpui::size(px(width), px(640.)));
        cx.run_until_parked();
        let toolbar = cx.debug_bounds("diff-toolbar").unwrap();
        let revisions = cx.debug_bounds("diff-revisions").unwrap();
        let body = cx.debug_bounds("diff-body").unwrap();
        for selector in [
            "diff-previous",
            "diff-next",
            "diff-file-previous",
            "diff-file-next",
            "diff-file-counter",
            "diff-edit",
            "diff-side-by-side",
            "diff-unified",
        ] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(
                bounds.left() >= toolbar.left() && bounds.right() <= toolbar.right(),
                "{selector} clipped at {width}"
            );
            assert!(
                bounds.top() >= toolbar.top() && bounds.bottom() <= toolbar.bottom(),
                "{selector} outside toolbar at {width}"
            );
        }
        tab.update(cx, |t, cx| {
            t.toggle_mode(cx);
            t.show_inspection(
                std::path::Path::new("/tmp"),
                format!("loading-{width}.txt").into(),
                "new-revision".into(),
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(cx.debug_bounds("diff-toolbar").unwrap(), toolbar);
        assert_eq!(cx.debug_bounds("diff-revisions").unwrap(), revisions);
        assert_eq!(cx.debug_bounds("diff-body").unwrap(), body);
    }
}
