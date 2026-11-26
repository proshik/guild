//! Real read-only source renderers, reached through their public lifecycle.
use gpui::{AppContext, TestAppContext, px};
use guild_desktop::{
    annotate_tab::AnnotateTab,
    diff_tab::{DiffTab, State},
    file_tab::Look,
    git::blame::Run,
    i18n::Ui,
    theme::{Kind, Theme},
};
use guild_session::{DiffView, Language};
fn look(theme: Theme) -> Look {
    Look {
        family: "Menlo".into(),
        font_size: 14.,
        theme,
        autosave: false,
    }
}
fn fills(cx: &mut gpui::VisualTestContext, theme: Theme) -> usize {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        let color: gpui::Hsla = theme.identifier_occurrence().into();
        window
            .painted_quads()
            .iter()
            .filter(|q| q.background.as_solid() == Some(color))
            .count()
    })
}
#[gpui::test]
fn restored_annotate_clicks_share_file_snapshot_and_replacement_clears(cx: &mut TestAppContext) {
    let mut owner = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let tab = cx.new(|cx| {
            AnnotateTab::new(
                "test.txt".into(),
                look(Theme::of(Kind::Light)),
                Ui(Language::English),
                cx,
            )
        });
        tab.update(cx, |tab, cx| {
            tab.set_runs(
                Ok(vec![Run {
                    sha: "a".repeat(40),
                    author: "fixture".into(),
                    time: 1,
                    summary: "test".into(),
                    first_line: 1,
                    original_line: 1,
                    filename: "test.txt".into(),
                    lines: vec!["имя имя_2".into(), "имя Имя".into()],
                }]),
                cx,
            )
        });
        owner = Some(tab.clone());
        gpui_component::Root::new(tab, window, cx)
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("source-snippet").unwrap();
    cx.simulate_click(
        gpui::point(bounds.left() + px(3.), bounds.center().y),
        Default::default(),
    );
    assert_eq!(fills(cx, Theme::of(Kind::Light)), 2);
    let owner = owner.unwrap();
    cx.update(|_, cx| {
        owner.update(cx, |tab, cx| {
            tab.restyle(look(Theme::of(Kind::Dark)), Ui(Language::English), cx)
        })
    });
    assert_eq!(fills(cx, Theme::of(Kind::Dark)), 2);
    cx.update(|_, cx| owner.update(cx, |tab, cx| tab.set_runs(Ok(vec![]), cx)));
    assert_eq!(fills(cx, Theme::of(Kind::Dark)), 0);
}
#[gpui::test]
fn proposal_diff_uses_raw_source_sides_unified_and_invalidation(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let mut owner = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        guild_desktop::actions::install(cx);
        let tab = cx.new(|cx| {
            DiffTab::open_proposal(
                std::path::Path::new("/tmp"),
                "test.txt".into(),
                Some("имя имя\r\n".as_bytes().to_vec()),
                "имя; имя имя_2\rимя\n".as_bytes().to_vec(),
                DiffView::SideBySide,
                look(Theme::of(Kind::Light)),
                Ui(Language::English),
                cx,
            )
        });
        owner = Some(tab.clone());
        gpui_component::Root::new(tab, window, cx)
    });
    let owner = owner.unwrap();
    settle(&owner, cx);
    let bounds = cx.debug_bounds("diff-source-1-1").expect("new source row");
    cx.simulate_click(
        gpui::point(bounds.left() + px(3.), bounds.center().y),
        Default::default(),
    );
    assert_eq!(fills(cx, Theme::of(Kind::Light)), 5);
    cx.update(|_, cx| owner.update(cx, |tab, cx| tab.toggle_mode(cx)));
    assert_eq!(fills(cx, Theme::of(Kind::Light)), 5);
    cx.update(|_, cx| {
        owner.update(cx, |tab, cx| {
            tab.restyle(look(Theme::of(Kind::Dark)), Ui(Language::English), cx)
        })
    });
    assert_eq!(fills(cx, Theme::of(Kind::Dark)), 5);
    cx.simulate_keystrokes("cmd-a");
    assert_eq!(fills(cx, Theme::of(Kind::Dark)), 0);
    cx.update(|_, cx| {
        owner.update(cx, |tab, cx| {
            tab.set_working("test.txt".into(), Some(b"other other\n".to_vec()), cx)
        })
    });
    settle(&owner, cx);
    assert_eq!(fills(cx, Theme::of(Kind::Dark)), 0);
}
fn settle(tab: &gpui::Entity<DiffTab>, cx: &mut gpui::VisualTestContext) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if cx.update(|_, cx| {
            tab.update(cx, |tab, cx| {
                tab.poll(cx);
                tab.files().iter().all(|f| f.state != State::Loading)
            })
        }) {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    cx.run_until_parked();
}

#[gpui::test]
fn diff_context_expansion_reveals_mentions_from_the_same_source(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;
    use guild_desktop::{diff::document::DisplayRow, diff_tab::Line};
    cx.executor().allow_parking();
    let mut owner = None;
    let before = format!("{}old\n", "name\r\n".repeat(25));
    let after = format!("{}new name\n", "name\r\n".repeat(25));
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let tab = cx.new(|cx| {
            DiffTab::open_proposal(
                std::path::Path::new("/tmp"),
                "context.txt".into(),
                Some(before.into_bytes()),
                after.into_bytes(),
                DiffView::SideBySide,
                look(Theme::of(Kind::Light)),
                Ui(Language::English),
                cx,
            )
        });
        owner = Some(tab.clone());
        gpui_component::Root::new(tab, window, cx)
    });
    let owner = owner.unwrap();
    settle(&owner, cx);
    let (source, fold) = cx.update(|_, cx| {
        let tab = owner.read(cx);
        let source = tab
            .lines()
            .iter()
            .position(|line| {
                matches!(
                    line,
                    Line::Source {
                        row: DisplayRow::Text { new: Some(_), .. },
                        ..
                    }
                )
            })
            .unwrap();
        let fold = tab
            .lines()
            .iter()
            .position(|line| {
                matches!(
                    line,
                    Line::Source {
                        row: DisplayRow::Fold { .. },
                        ..
                    }
                )
            })
            .unwrap();
        (source, fold)
    });
    assert_eq!(source, 2, "fixture has a leading context fold");
    let bounds = cx.debug_bounds("diff-source-1-2").unwrap();
    cx.simulate_click(
        gpui::point(bounds.left() + px(3.), bounds.center().y),
        Default::default(),
    );
    let folded = fills(cx, Theme::of(Kind::Light));
    assert!(folded > 0 && folded < 51);
    cx.update(|window, cx| window.click(("diff-fold", fold), cx));
    cx.simulate_resize(gpui::size(px(1000.), px(1600.)));
    assert_eq!(fills(cx, Theme::of(Kind::Light)), 51);
}
