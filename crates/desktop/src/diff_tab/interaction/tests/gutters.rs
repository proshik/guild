use super::*;

#[gpui::test]
fn old_inner_gutter_stays_fixed_while_tail_selection_uses_the_text_viewport(
    cx: &mut TestAppContext,
) {
    let prefix = "Левая длинная строка ".repeat(60);
    let tail = "ЛЕВЫЙ_ХВОСТ_Ж🧭";
    let old = format!("{prefix}{tail}\n");
    let (tab, cx) = fixture(cx, &old, "new\n");
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let hit = cx.update(|_, cx| {
        tab.read(cx)
            .viewer
            .hits
            .borrow()
            .iter()
            .find(|h| h.side == 0)
            .unwrap()
            .clone()
    });
    assert!(
        hit.bounds.left() < px(5.),
        "old code must begin at its pane's left, not after a leading gutter"
    );
    let gutter = cx.debug_bounds("diff-numbers-old-1").unwrap();
    assert_eq!(
        hit.bounds.right(),
        cx.debug_bounds("diff-blame-strip-old-1").unwrap().left()
            - cx.update(|window, _| window.rem_size()) * crate::theme::space::TREE_GAP.0 * 2.
    );
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: hit.bounds.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(-20000.), px(0.))),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("diff-numbers-old-1").unwrap(), gutter);
    let (hit, offset) = cx.update(|_, cx| {
        let tab = tab.read(cx);
        (
            tab.viewer
                .hits
                .borrow()
                .iter()
                .find(|h| h.side == 0)
                .unwrap()
                .clone(),
            tab.viewer.horizontal[0].0.borrow().offset,
        )
    });
    let from = point(
        hit.bounds.left() + hit.shaped.x_for_index(prefix.len()) - offset,
        hit.bounds.center().y,
    );
    let to = point(
        hit.bounds.left() + hit.shaped.x_for_index(prefix.len() + tail.len()) - offset,
        hit.bounds.center().y,
    );
    assert!(hit.bounds.contains(&from) && hit.bounds.contains(&to));
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, Default::default());
    cx.simulate_keystrokes("cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("ЛЕВЫЙ_ХВОСТ_Ж🧭")
        )
    });
}

#[gpui::test]
fn overview_marks_cover_blocks_and_click_their_logical_owner(cx: &mut TestAppContext) {
    let old = format!("start\n{}end\n", "old\n".repeat(40));
    let new = format!("start\n{}end\n", "new\n".repeat(55));
    let (tab, cx) = fixture(cx, &old, &new);
    cx.simulate_resize(gpui::size(px(1000.), px(650.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let stripe = cx
        .debug_bounds("diff-overview-stripe")
        .expect("a separate change stripe beside the usable scrollbar");
    assert!(
        stripe.right() <= cx.update(|window, _| window.viewport_size().width),
        "overview rail must be inside the actual viewport: {stripe:?}"
    );
    let mark = cx.debug_bounds("diff-overview-mark-0").unwrap();
    assert!(
        mark.size.height > stripe.size.height * 0.7,
        "large replacement is a range, not a header tick"
    );
    cx.simulate_click(mark.center(), Default::default());
    cx.update(|_, cx| assert_eq!(tab.read(cx).viewer.active_change, Some((0, 0))));
    let scrollbar = cx.debug_bounds("diff-scrollbar-lane").unwrap();
    assert!(stripe.right() <= scrollbar.left());
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    // Drag the real adjacent scrollbar after selecting a mark. The two hit
    // regions must stay independent and manual scrolling clears logical owner.
    cx.simulate_mouse_down(
        point(scrollbar.center().x, scrollbar.top() + px(35.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(scrollbar.center().x, scrollbar.bottom() - px(12.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_up(
        point(scrollbar.center().x, scrollbar.bottom() - px(12.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.update(|_, cx| {
        assert!(tab.read(cx).viewer.active_change.is_none());
        assert!(tab.read(cx).scroll.offset().y < px(-100.));
    });
}

#[test]
fn revision_blame_reads_the_old_renamed_path_instead_of_head() {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(dir.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    std::fs::write(dir.path().join("old.txt"), "first author\nstable context\n").unwrap();
    git(&["add", "old.txt"]);
    git(&[
        "-c",
        "commit.gpgsign=false",
        "-c",
        "user.name=Alice Original",
        "-c",
        "user.email=a@example.invalid",
        "commit",
        "-qm",
        "baseline",
    ]);
    let old = git(&["rev-parse", "HEAD"]);
    std::fs::rename(dir.path().join("old.txt"), dir.path().join("new.txt")).unwrap();
    std::fs::write(
        dir.path().join("new.txt"),
        "second author\nstable context\n",
    )
    .unwrap();
    git(&["add", "old.txt", "new.txt"]);
    git(&[
        "-c",
        "commit.gpgsign=false",
        "-c",
        "user.name=Bob Current",
        "-c",
        "user.email=b@example.invalid",
        "commit",
        "-qm",
        "rename and edit",
    ]);
    let bytes = crate::git::blame_at(dir.path(), &old, std::path::Path::new("old.txt")).unwrap();
    let runs = crate::git::blame::parse(&bytes).unwrap();
    assert_eq!(runs[0].author, "Alice Original");
    assert_eq!(runs[0].sha, old);
    let bytes = crate::git::blame_at(dir.path(), "HEAD", std::path::Path::new("new.txt")).unwrap();
    assert_eq!(
        crate::git::blame::parse(&bytes).unwrap()[0].author,
        "Bob Current"
    );
    assert!(git(&["status", "--porcelain"]).is_empty());
}

#[gpui::test]
fn authors_context_menu_is_independent_and_proposals_never_invent_attribution(
    cx: &mut TestAppContext,
) {
    let (tab, cx) = fixture(cx, "old\n", "new\nmore\n");
    cx.run_until_parked();
    let old = cx.debug_bounds("diff-numbers-old-1").unwrap();
    cx.simulate_mouse_down(old.center(), gpui::MouseButton::Right, Default::default());
    cx.simulate_mouse_up(old.center(), gpui::MouseButton::Right, Default::default());
    cx.run_until_parked();
    let toggle = cx
        .debug_bounds("diff-authors-toggle")
        .expect("actual gutter context menu");
    cx.simulate_click(toggle.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let t = tab.read(cx);
        assert_eq!(t.authors.enabled, [true, false]);
        assert!(matches!(
            t.authors.entries[&(PathBuf::from("review.txt"), 0)].data,
            super::super::super::AuthorData::Unavailable(_)
        ));
        assert!(t.author_identity(0, 0).is_none());
    });
    let numbers = cx.debug_bounds("diff-numbers-old-1").unwrap();
    let annotation = cx.debug_bounds("diff-authors-0-1").unwrap();
    assert_eq!(numbers.right(), annotation.left());
    assert_eq!(
        annotation.right(),
        old.right(),
        "annotation remains nearest the connector"
    );
    let right = cx.debug_bounds("diff-numbers-new-1").unwrap();
    cx.simulate_mouse_down(right.center(), gpui::MouseButton::Right, Default::default());
    cx.simulate_mouse_up(right.center(), gpui::MouseButton::Right, Default::default());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(tab.read(cx).authors.enabled, [true, false]);
        assert!(tab.read(cx).focus.is_focused(window));
    });
    tab.update(cx, |t, cx| t.toggle_mode(cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tab.read(cx).authors.enabled, [true, false]));
    tab.update(cx, |t, cx| {
        t.show_inspection(
            std::path::Path::new("/tmp"),
            "another.txt".into(),
            "another-revision".into(),
            cx,
        )
    });
    cx.update(|_, cx| {
        assert_eq!(
            tab.read(cx).authors.enabled,
            [true, false],
            "stepping retains the per-side opt-in"
        )
    });
}

#[test]
fn source_authors_reuse_equal_line_mapping_without_cr_or_uncommitted_fabrication() {
    let run = crate::git::blame::Run {
        sha: "a".repeat(40),
        author: "Alice Original".into(),
        time: 0,
        summary: "fixture".into(),
        first_line: 1,
        original_line: 7,
        filename: "old.txt".into(),
        lines: vec!["a\rb\r".into(), "c".into()],
    };
    let mapped = super::super::super::map_source_authors(std::slice::from_ref(&run), "a\rb\r\nc");
    assert_eq!(mapped.len(), 3);
    assert_eq!(
        mapped
            .iter()
            .flatten()
            .map(|line| line.original_line)
            .collect::<Vec<_>>(),
        vec![7, 7, 8]
    );
    assert_eq!(mapped[0].as_ref().unwrap().compact(), "01.01.1970 Original");
    let mapped =
        super::super::super::map_source_authors(std::slice::from_ref(&run), "a\rchanged\r\nc");
    assert!(mapped[1].is_none());
    let mut uncommitted = run;
    uncommitted.sha = "0".repeat(40);
    assert!(
        super::super::super::map_source_authors(&[uncommitted], "a\rb\r\nc")
            .iter()
            .all(Option::is_none)
    );
}

#[gpui::test]
fn overview_uses_projected_ranges_for_insert_delete_replace_and_unified(cx: &mut TestAppContext) {
    let middle = (0..70)
        .map(|i| format!("unchanged-{i}\n"))
        .collect::<String>();
    let (tab, cx) = fixture(
        cx,
        &format!("gone\n{middle}old-a\nold-b\nend\n"),
        &format!("{middle}new\nend\ninserted\n"),
    );
    tab.update(cx, |t, cx| {
        let ranges = t.overview_ranges();
        assert_eq!(
            ranges.iter().map(|range| range.kind).collect::<Vec<_>>(),
            vec![
                super::super::super::HunkKind::Removed,
                super::super::super::HunkKind::Changed,
                super::super::super::HunkKind::Added
            ]
        );
        assert_eq!(
            ranges
                .iter()
                .map(|range| range.rows.len())
                .collect::<Vec<_>>(),
            vec![1, 2, 1]
        );
        for (range, target) in ranges.iter().zip(t.change_targets()) {
            assert_eq!((range.file, range.block, range.rows.start), target);
        }
        let folded = t.lines.len();
        t.expand_context(0, 0, cx);
        assert!(t.lines.len() > folded);
        assert_eq!(
            t.overview_ranges()
                .iter()
                .map(|range| range.rows.len())
                .collect::<Vec<_>>(),
            vec![1, 2, 1]
        );
        t.toggle_mode(cx);
        assert_eq!(
            t.overview_ranges()
                .iter()
                .map(|range| range.rows.len())
                .collect::<Vec<_>>(),
            vec![1, 3, 1]
        );
        let (top, length) = super::super::super::overview_span(&(999..1000), 1000, 0.01);
        assert!(top >= 0. && top + length <= 1.);
        assert!(length >= 0.0099);
    });
}

#[gpui::test]
fn author_answers_reject_old_generation_revision_snapshot_and_disabled_side(
    cx: &mut TestAppContext,
) {
    use super::super::super::{AuthorData, AuthorEntry, AuthorIdentity};
    let (tab, cx) = fixture(cx, "old\n", "new\n");
    tab.update(cx, |t, cx| {
        let path = PathBuf::from("review.txt");
        let source = t.documents[&path].sources[0].text.clone();
        let identity = AuthorIdentity {
            root: t.root.clone(), revision: "historical-a".into(), path: "old-name.txt".into(),
            repository_relative: true, fetch_generation: t.fetch_generation.get(),
        };
        t.authors.enabled[0] = true;
        let reset = |t: &mut DiffTab| {
            t.authors.entries.insert((path.clone(), 0), AuthorEntry {
                identity: Some(identity.clone()), source: source.clone(), generation: 7,
                data: AuthorData::Loading,
                width: None,
            });
        };
        reset(t);
        t.authors_landed(path.clone(), 0, 6, identity.clone(), source.clone(), Ok(vec![None]), cx);
        assert!(matches!(t.authors.entries[&(path.clone(), 0)].data, AuthorData::Loading));
        let mut renamed = identity.clone();
        renamed.path = "wrong-name.txt".into();
        t.authors_landed(path.clone(), 0, 7, renamed, source.clone(), Ok(vec![None]), cx);
        assert!(matches!(t.authors.entries[&(path.clone(), 0)].data, AuthorData::Loading));
        let wrong_snapshot = std::sync::Arc::<str>::from(source.as_ref());
        t.authors_landed(path.clone(), 0, 7, identity.clone(), wrong_snapshot, Ok(vec![None]), cx);
        assert!(matches!(t.authors.entries[&(path.clone(), 0)].data, AuthorData::Loading));
        let generation = t.fetch_generation.get();
        t.fetch_generation.set(generation + 1);
        t.authors_landed(path.clone(), 0, 7, identity.clone(), source.clone(), Ok(vec![None]), cx);
        assert!(matches!(t.authors.entries[&(path.clone(), 0)].data, AuthorData::Loading));
        t.fetch_generation.set(generation);
        t.authors.enabled[0] = false;
        t.authors_landed(path.clone(), 0, 7, identity.clone(), source.clone(), Ok(vec![None]), cx);
        assert!(matches!(t.authors.entries[&(path.clone(), 0)].data, AuthorData::Loading));
        t.authors.enabled[0] = true;
        t.authors_landed(path.clone(), 0, 7, identity, source, Ok(vec![None]), cx);
        assert!(matches!(&t.authors.entries[&(path, 0)].data, AuthorData::Ready(lines) if lines.len() == 1));
    });
}

#[gpui::test]
fn authors_gutters_reserve_source_and_scrollbar_geometry_in_both_modes(cx: &mut TestAppContext) {
    let text = format!("{}TAIL\n", "long source ".repeat(120));
    let (tab, cx) = fixture(cx, &text, &text.replace("source", "replacement"));
    cx.simulate_resize(gpui::size(px(1000.), px(420.)));
    tab.update(cx, |t, cx| {
        t.toggle_authors(0, cx);
        t.toggle_authors(1, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let old_numbers = cx.debug_bounds("diff-numbers-old-1").unwrap();
    let old_authors = cx.debug_bounds("diff-authors-0-1").unwrap();
    let new_numbers = cx.debug_bounds("diff-numbers-new-1").unwrap();
    let new_authors = cx.debug_bounds("diff-authors-1-1").unwrap();
    assert_eq!(
        old_numbers.right(),
        old_authors.left(),
        "old code → numbers → blame → connectors"
    );
    assert_eq!(
        new_authors.right(),
        new_numbers.left(),
        "connectors → blame → numbers → new code"
    );
    let bounds = cx.update(|_, cx| tab.read(cx).viewer.horizontal[0].0.borrow().bounds);
    assert!(bounds.right() <= old_numbers.left());
    let start = point(bounds.left() + px(5.), bounds.center().y);
    let end = point(bounds.right() - px(2.), bounds.center().y);
    // Horizontal handle geometry is the code viewport; the track is beneath it.
    let bottom = cx.update(|_, cx| tab.read(cx).viewer.viewport.bottom());
    cx.simulate_mouse_down(
        point(start.x, bottom + px(4.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(end.x, bottom + px(4.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_up(
        point(end.x, bottom + px(4.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.update(|_, cx| {
        let h = tab.read(cx).viewer.horizontal[0].0.borrow();
        assert!(h.offset >= (h.width - h.bounds.size.width) * 0.95);
    });
    assert_eq!(cx.debug_bounds("diff-numbers-old-1").unwrap(), old_numbers);
    for width in [320., 160.] {
        cx.simulate_resize(gpui::size(px(width), px(420.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.update(|_, cx| {
            let t = tab.read(cx);
            assert!(t.viewer.minimum_ratio <= t.viewer.maximum_ratio);
            assert!(
                t.viewer
                    .hits
                    .borrow()
                    .iter()
                    .all(|hit| hit.bounds.size.width >= px(0.))
            );
        });
    }
    tab.update(cx, |t, cx| t.toggle_mode(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let old = cx.debug_bounds("diff-numbers-old-1").unwrap();
    let new = cx.debug_bounds("diff-numbers-new-1").unwrap();
    assert_eq!(
        old.right(),
        new.left(),
        "unified retains its logical leading pair"
    );
    cx.update(|_, cx| {
        assert!(
            tab.read(cx)
                .viewer
                .hits
                .borrow()
                .iter()
                .all(|hit| hit.bounds.left() >= new.right())
        );
    });
}

#[gpui::test]
fn author_gutters_fit_full_file_labels_and_follow_the_final_reference(cx: &mut TestAppContext) {
    let old = "old\n".repeat(80);
    let new = "new\n".repeat(80);
    let (tab, cx) = fixture(cx, &old, &new);
    cx.simulate_resize(gpui::size(px(1200.), px(440.)));
    tab.update(cx, |t, cx| {
        t.toggle_authors(0, cx);
        t.toggle_authors(1, cx);
    });
    cx.run_until_parked();
    let author = crate::git::blame::Line {
        sha: "a".repeat(40),
        author: "Alice Fixture".into(),
        time: 0,
        summary: "fixture".into(),
        filename: "review.txt".into(),
        original_line: 1,
    };
    let mut lines = vec![Some(author); 80];
    for (index, line) in lines.iter_mut().enumerate() {
        line.as_mut().unwrap().time = ((index % 6 + 1) * 100) as i64;
    }
    // The widest label is offscreen. Scrolling must not resize the gutter.
    lines[79].as_mut().unwrap().author = "Alice Lastauthor".into();
    tab.update(cx, |t, cx| {
        for side in 0..2 {
            t.authors
                .entries
                .get_mut(&(PathBuf::from("review.txt"), side))
                .unwrap()
                .data = super::super::super::AuthorData::Ready(lines.clone());
            t.authors
                .entries
                .get_mut(&(PathBuf::from("review.txt"), side))
                .unwrap()
                .width = None;
        }
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let old_author = cx.debug_bounds("diff-authors-0-1").unwrap();
    let new_author = cx.debug_bounds("diff-authors-1-1").unwrap();
    let old_number = cx.debug_bounds("diff-numbers-old-1").unwrap();
    let new_number = cx.debug_bounds("diff-numbers-new-1").unwrap();
    assert!(
        old_author.size.width < px(180.),
        "short date/author labels must not reserve the fixed 200px cap: {old_author:?}"
    );
    assert_eq!(old_author.size.width, new_author.size.width);
    assert_eq!(
        old_number.right(),
        old_author.left(),
        "old code → numbers → blame → connectors"
    );
    assert_eq!(
        new_author.right(),
        new_number.left(),
        "connectors → blame → numbers → new code"
    );
    cx.update(|window, _| {
        let label = "01.01.1970 Lastauthor";
        let mut font = window.text_style().font();
        font.weight=gpui::FontWeight::NORMAL;
        let size = crate::theme::size::META.to_pixels(window.rem_size());
        let shaped = window.text_system().shape_line(label.into(), size, &[gpui::TextRun {
            len: label.len(), font, color: gpui::black(), background_color: None,
            underline: None, strikethrough: None,
        }], None);
        let padded = shaped.width + crate::theme::space::CARD_GAP.to_pixels(window.rem_size()) * 2.;
        assert!((f32::from(old_author.size.width - padded)).abs() <= 1.,
            "fit the widest actual full-file label with editor padding, not a viewport-sized reserve");
    });
    cx.update(|_, cx| {
        let t = tab.read(cx);
        for side in 0..2 {
            let cache = t.authors.entries[&(PathBuf::from("review.txt"), side)].width.as_ref().unwrap();
            assert_eq!([100,200,300,400,500,600].map(|time| cache.shades[&time]), [2,2,1,1,0,0],
                "blame backgrounds use the existing chronological thirds, not the diff tag or current date");
        }
    });
    let full_width = old_author.size.width;
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: old_author.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-10000.))),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let last = cx.debug_bounds("diff-authors-0-80").unwrap();
    assert_eq!(
        last.size.width, full_width,
        "offscreen labels were included before scrolling"
    );
    let last_new = cx.debug_bounds("diff-numbers-new-80").unwrap();
    cx.simulate_mouse_down(
        last_new.center(),
        gpui::MouseButton::Right,
        Default::default(),
    );
    cx.simulate_mouse_up(
        last_new.center(),
        gpui::MouseButton::Right,
        Default::default(),
    );
    cx.run_until_parked();
    let menu = cx.debug_bounds("diff-authors-toggle").unwrap();
    cx.simulate_click(menu.center(), Default::default());
    cx.update(|_, cx| assert_eq!(tab.read(cx).authors.enabled, [true, false]));
}

#[gpui::test]
fn final_gutter_order_for_both_toggles_preserves_scrolled_copy_and_unified(
    cx: &mut TestAppContext,
) {
    let old_prefix = "Длинная старая строка ".repeat(90);
    let new_prefix = "Long replacement source ".repeat(90);
    let old = format!("{old_prefix}OLD_ХВОСТ_🧭\n");
    let new = format!("{new_prefix}NEW_é_👩‍💻\n");
    let (tab, cx) = fixture(cx, &old, &new);
    cx.simulate_resize(gpui::size(px(1100.), px(450.)));
    for enabled in [[false, false], [true, false], [false, true], [true, true]] {
        tab.update(cx, |t, cx| {
            for (side, enabled) in enabled.into_iter().enumerate() {
                if t.authors.enabled[side] != enabled {
                    t.toggle_authors(side, cx);
                }
            }
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        let left = cx.debug_bounds("diff-numbers-old-1").unwrap();
        let right = cx.debug_bounds("diff-numbers-new-1").unwrap();
        let left_auth = cx.debug_bounds("diff-authors-0-1");
        let right_auth = cx.debug_bounds("diff-authors-1-1");
        assert_eq!(left_auth.is_some(), enabled[0]);
        assert_eq!(right_auth.is_some(), enabled[1]);
        if let Some(auth) = left_auth {
            assert_eq!(left.right(), auth.left(), "old numbers → old blame");
        }
        if let Some(auth) = right_auth {
            assert_eq!(auth.right(), right.left(), "new blame → new numbers");
        }
        let marker =
            cx.update(|window, _| crate::theme::space::TREE_GAP.to_pixels(window.rem_size()) * 2.);
        let group_left_end = left_auth.map_or(left.right(), |auth| auth.right());
        let group_right_start = right_auth.map_or(right.left(), |auth| auth.left());
        assert_eq!(
            group_right_start - group_left_end,
            cx.update(|window, _| crate::theme::space::DIFF_CONNECTOR.to_pixels(window.rem_size())),
            "only the central connector separates blame groups"
        );
        let strip_width = cx
            .debug_bounds("diff-blame-strip-old-1")
            .unwrap()
            .size
            .width;
        cx.update(|_, cx| {
            let t = tab.read(cx);
            let hits = t.viewer.hits.borrow();
            let old = hits.iter().find(|hit| hit.side == 0).unwrap();
            let new = hits.iter().find(|hit| hit.side == 1).unwrap();
            assert_eq!(
                old.bounds.right() + marker + strip_width,
                left.left(),
                "old text/marker → numbers → optional blame"
            );
            assert_eq!(
                right.right() + strip_width + marker,
                new.bounds.left(),
                "optional blame → numbers → new marker/text"
            );
            assert_eq!(t.viewer.horizontal[0].0.borrow().bounds, old.bounds);
            assert_eq!(t.viewer.horizontal[1].0.borrow().bounds, new.bounds);
        });
    }
    for (side, prefix, tail) in [(0, old_prefix, "OLD_ХВОСТ_🧭"), (1, new_prefix, "NEW_é_👩‍💻")]
    {
        let numbers = cx
            .debug_bounds(if side == 0 {
                "diff-numbers-old-1"
            } else {
                "diff-numbers-new-1"
            })
            .unwrap();
        let hit = cx.update(|_, cx| {
            tab.read(cx)
                .viewer
                .hits
                .borrow()
                .iter()
                .find(|h| h.side == side)
                .unwrap()
                .clone()
        });
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: hit.bounds.center(),
            delta: gpui::ScrollDelta::Pixels(point(px(-30000.), px(0.))),
            modifiers: Default::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        cx.run_until_parked();
        assert_eq!(
            cx.debug_bounds(if side == 0 {
                "diff-numbers-old-1"
            } else {
                "diff-numbers-new-1"
            })
            .unwrap(),
            numbers
        );
        let (hit, offset) = cx.update(|_, cx| {
            let t = tab.read(cx);
            (
                t.viewer
                    .hits
                    .borrow()
                    .iter()
                    .find(|h| h.side == side)
                    .unwrap()
                    .clone(),
                t.viewer.horizontal[side].0.borrow().offset,
            )
        });
        let from = point(
            hit.bounds.left() + hit.shaped.x_for_index(prefix.len()) - offset,
            hit.bounds.center().y,
        );
        let to = point(
            hit.bounds.left() + hit.shaped.x_for_index(prefix.len() + tail.len()) - offset,
            hit.bounds.center().y,
        );
        assert!(hit.bounds.contains(&from) && hit.bounds.contains(&to));
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, Default::default());
        cx.simulate_keystrokes("cmd-c");
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(tail)
            )
        });
    }
    tab.update(cx, |t, cx| t.toggle_mode(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let left = cx.debug_bounds("diff-numbers-old-1").unwrap();
    let right = cx.debug_bounds("diff-numbers-new-1").unwrap();
    assert_eq!(
        left.right(),
        right.left(),
        "unified keeps logical leading old/new numbers"
    );
    cx.update(|_, cx| assert_eq!(tab.read(cx).authors.enabled, [true, true]));
}

#[gpui::test]
fn blame_click_strips_are_symmetric_and_open_the_same_menu_in_all_states(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old\n", "new\n");
    cx.simulate_resize(gpui::size(px(1100.), px(450.)));
    for enabled in [[false, false], [true, false], [false, true], [true, true]] {
        tab.update(cx, |t, cx| {
            for (side, enabled) in enabled.iter().enumerate() {
                if t.authors.enabled[side] != *enabled {
                    t.toggle_authors(side, cx);
                }
            }
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        let old = cx
            .debug_bounds("diff-blame-strip-old-1")
            .expect("explicit old blame click strip");
        let new = cx
            .debug_bounds("diff-blame-strip-new-1")
            .expect("explicit new blame click strip");
        let old_number = cx.debug_bounds("diff-numbers-old-1").unwrap();
        let new_number = cx.debug_bounds("diff-numbers-new-1").unwrap();
        assert_eq!(
            old.size, new.size,
            "equal strips on either side, independent of authors"
        );
        assert!(old.size.width > px(0.));
        assert_eq!(
            old.right(),
            old_number.left(),
            "old code → click strip → numbers → optional blame"
        );
        assert_eq!(
            new_number.right(),
            new.left(),
            "optional blame → numbers → click strip → new code"
        );
        for (side, strip) in [(0, old), (1, new)] {
            for button in [gpui::MouseButton::Left, gpui::MouseButton::Right] {
                cx.simulate_mouse_down(strip.center(), button, Default::default());
                cx.simulate_mouse_up(strip.center(), button, Default::default());
                cx.run_until_parked();
                assert!(cx.debug_bounds("diff-authors-toggle").is_some());
                cx.update(|_, cx| assert_eq!(tab.read(cx).authors_menu.as_ref().unwrap().0, side));
                let toggle = cx.debug_bounds("diff-authors-toggle").unwrap();
                cx.simulate_click(toggle.center(), Default::default());
                cx.run_until_parked();
                cx.update(|_, cx| {
                    let expected = if side == 0 {
                        [!enabled[0], enabled[1]]
                    } else {
                        [enabled[0], !enabled[1]]
                    };
                    assert_eq!(tab.read(cx).authors.enabled, expected);
                });
                tab.update(cx, |t, cx| t.toggle_authors(side, cx));
                cx.run_until_parked();
            }
        }
    }
}

#[gpui::test]
fn number_width_tracks_full_source_digits_and_font_without_losing_strip_targets(
    cx: &mut TestAppContext,
) {
    use gpui_kit::test::TestWindowExt;
    fn check(cx: &mut TestAppContext, old_count: usize, new_count: usize) {
        let old = "same\n".repeat(old_count);
        let new = "same\n".repeat(new_count);
        let (tab, cx) = fixture(cx, &old, &new);
        let mut prior = px(0.);
        for size in [12.5, 20.] {
            tab.update(cx, |t, cx| {
                let mut look = t.look().clone();
                look.font_size = size;
                t.restyle(look, t.ui(), cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.simulate_next_frame(cx));
            let index = cx.update(|_, cx| {
                tab.read(cx)
                    .lines()
                    .iter()
                    .position(|line| {
                        matches!(
                            line,
                            Line::Source {
                                row: crate::diff::document::DisplayRow::Text {
                                    old: Some(_),
                                    new: Some(_),
                                    ..
                                },
                                ..
                            }
                        )
                    })
                    .unwrap()
            });
            let old = cx.update(|window, _| window.find(("diff-numbers-old", index)).bounds());
            let new = cx.update(|window, _| window.find(("diff-numbers-new", index)).bounds());
            assert_eq!(
                old.size.width, new.size.width,
                "common width across digit boundary"
            );
            let (label, padding) = cx.update(|window, _| {
                let label = crate::file_tab::gutter::text_width(
                    &new_count.to_string(),
                    px(size),
                    gpui::Font {
                        family: "Menlo".into(),
                        ..window.text_style().font()
                    },
                    window,
                );
                (
                    label,
                    crate::theme::space::CARD_GAP.to_pixels(window.rem_size()),
                )
            });
            assert!(
                old.size.width >= label + padding,
                "maximum source label must not clip"
            );
            assert!(
                old.size.width <= label + padding + px(2.),
                "numbers must not reserve fixed blank width: {:?} vs {:?}",
                old.size.width,
                label + padding
            );
            assert!(old.size.width > prior, "font changes update measured width");
            prior = old.size.width;
            let left = cx.update(|window, _| window.find(("diff-blame-strip-old", index)).bounds());
            let right =
                cx.update(|window, _| window.find(("diff-blame-strip-new", index)).bounds());
            assert_eq!(left.size.width, right.size.width);
            assert_eq!(left.right(), old.left());
            assert_eq!(new.right(), right.left());
            cx.simulate_click(right.center(), Default::default());
            cx.run_until_parked();
            assert!(cx.debug_bounds("diff-authors-toggle").is_some());
            cx.simulate_keystrokes("escape");
            tab.update(cx, |t, cx| t.expand_context(0, 0, cx));
            cx.run_until_parked();
            cx.update(|window, cx| window.simulate_next_frame(cx));
            let index = cx.update(|_, cx| {
                tab.read(cx)
                    .lines()
                    .iter()
                    .position(|line| {
                        matches!(
                            line,
                            Line::Source {
                                row: crate::diff::document::DisplayRow::Text {
                                    old: Some(_),
                                    new: Some(_),
                                    ..
                                },
                                ..
                            }
                        )
                    })
                    .unwrap()
            });
            assert_eq!(
                cx.update(|window, _| window.find(("diff-numbers-old", index)).bounds())
                    .size
                    .width,
                prior,
                "expansion cannot change source-based width"
            );
        }
    }
    for (old, new) in [(9, 10), (99, 100), (999, 1000)] {
        check(cx, old, new);
    }
}
