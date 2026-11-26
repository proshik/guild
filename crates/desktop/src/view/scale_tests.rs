//! Scaled production components, with a real local search and no external services.
use crate::{Root, theme};
use gpui::{AppContext, Context, Entity, Render, TestAppContext, Window, div, prelude::*, px};

#[test]
fn isolated_scale_contract() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::scale_tests::real_",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .env("GUILD_SCALE_TEST", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Components {
    root: Entity<Root>,
    page: bool,
}
impl Render for Components {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.root.update(cx, |root, cx| {
            window.set_rem_size(crate::scale::rem_size(root.settings.ui_font_size));
            if self.page {
                return div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .children(root.section_rows(super::settings::Section::Terminal, cx))
                    .into_any_element();
            }
            div()
                .size_full()
                .flex()
                .flex_col()
                .text_size(theme::size::BODY)
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .child(crate::icons::icon("rust").debug_selector(|| "file-icon".into()))
                        .child(crate::icons::chevron(true).debug_selector(|| "disclosure".into()))
                        .child(
                            crate::icons::icon("rust")
                                .size(px(19.))
                                .debug_selector(|| "overridden-icon".into()),
                        )
                        .child(
                            crate::icons::chevron(true)
                                .size(px(9.))
                                .debug_selector(|| "overridden-chevron".into()),
                        ),
                )
                .child(super::popup::heading(
                    "Длинная группа меню".into(),
                    root.theme,
                ))
                .children(root.render_search(cx))
                .child(root.render_status(window, cx))
                .into_any_element()
        })
    }
}
fn settle(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
}

#[gpui::test]
#[ignore = "run through isolated_scale_contract"]
fn real_scaled_icons_search_heading_fault_and_terminal_preview(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_SCALE_TEST").as_deref(), Ok("1"));
    cx.executor().allow_parking();
    let state = crate::paths::state_dir();
    let fixture = state.join("fixture");
    std::fs::create_dir_all(&fixture).unwrap();
    std::fs::write(
        fixture.join("sample.rs"),
        "fn main() {}\n// needle Привет\n// needle again\n",
    )
    .unwrap();
    let mut desk = guild_session::Desk::default();
    let project = desk.open_in(desk.active_workspace(), &fixture);
    desk.look_at(Some(project), None);
    let mut made = None;
    let (host, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| Root::diagnostic(desk, state, cx));
        root.update(cx, |root, cx| {
            root.isolated_render = true;
            root.ensure_search(window, cx);
            root.searches.get_mut(&project).unwrap().query.text = "needle".into();
            root.restart_search(project, cx);
            root.open_file(std::path::Path::new("sample.rs"), window, cx);
            root.scoped_settings.identities.insert(
                project,
                Ok(crate::repository_settings::resolve_repository(&fixture).unwrap()),
            );
            root.serverless
                .insert((project, "rust"), crate::lsp::progress::Fault::Missing);
        });
        made = Some(root.clone());
        Components { root, page: false }
    });
    let root = made.unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let done = root.update(cx, |root, cx| {
            root.poll_search(cx);
            !root.searches[&project].walk.running()
        });
        if done {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        root.read_with(cx, |root, _| root.searches[&project].walk.hits().len()),
        2
    );
    cx.simulate_resize(gpui::size(px(1200.), px(950.)));
    let mut failures = Vec::new();
    settle(cx);
    crate::search_tree::BUILDS.with(|n| n.set(0));
    cx.update(|window, cx| {
        let focus = root.read(cx).searches[&project].results_focus.clone();
        focus.focus(window, cx);
    });
    for _ in 0..20 {
        cx.simulate_keystrokes("down");
        host.update(cx, |_, cx| cx.notify());
        settle(cx);
    }
    let builds = crate::search_tree::BUILDS.with(|n| n.get());
    if builds != 0 {
        failures.push(format!(
            "selection-only redraw built the search tree {builds} times"
        ));
    }

    let snapshot = root.read_with(cx, |r, _| r.searches[&project].presentation());
    root.update(cx, |r, cx| r.toggle_search_fold("sample.rs".into(), cx));
    let folded = root.read_with(cx, |r, _| r.searches[&project].presentation());
    assert!(
        folded
            .rows
            .iter()
            .all(|row| !matches!(row, crate::search_tree::Row::Hit { .. }))
    );
    assert_eq!(
        snapshot.hits.len(),
        2,
        "an old paint snapshot stays immutable"
    );
    root.update(cx, |r, cx| r.toggle_search_fold("sample.rs".into(), cx));
    assert_eq!(
        root.read_with(cx, |r, _| r
            .search_rows()
            .iter()
            .filter(|row| matches!(row, crate::search_tree::Row::Hit { .. }))
            .count()),
        2
    );
    root.update(cx, |r, _| {
        r.relocate_search(
            project,
            std::path::Path::new("sample.rs"),
            std::path::Path::new("moved.rs"),
        )
    });
    assert!(root.read_with(cx, |r, _| {
        r.searches[&project]
            .presentation()
            .hits
            .iter()
            .all(|hit| hit.path == std::path::Path::new("moved.rs"))
    }));
    root.update(cx, |r, _| {
        r.relocate_search(
            project,
            std::path::Path::new("moved.rs"),
            std::path::Path::new("sample.rs"),
        )
    });
    root.update(cx, |r, cx| {
        r.searches.get_mut(&project).unwrap().query.text.clear();
        r.restart_search(project, cx);
    });
    assert!(root.read_with(cx, |r, _| r.search_rows().is_empty()));
    root.update(cx, |r, cx| {
        r.searches.get_mut(&project).unwrap().query.text = "needle".into();
        r.restart_search(project, cx);
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let done = root.update(cx, |r, cx| {
            r.poll_search(cx);
            !r.searches[&project].walk.running()
        });
        if done {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        root.read_with(cx, |r, _| r.searches[&project].presentation().hits.len()),
        2
    );

    for palette in [theme::Kind::Dark, theme::Kind::Light] {
        for ui in [10., 13., 16., 20.] {
            root.update(cx, |root, _| {
                root.settings.ui_font_size = ui;
                root.theme = theme::Theme::of(palette);
            });
            host.update(cx, |host, cx| {
                host.page = false;
                cx.notify();
            });
            settle(cx);
            for (name, expected) in [
                ("file-icon", 16. * ui / 13.),
                ("disclosure", 11. * ui / 13.),
                ("overridden-icon", 19.),
                ("overridden-chevron", 9.),
            ] {
                let bounds = cx.debug_bounds(name).unwrap();
                if (f32::from(bounds.size.height) - expected).abs() > 1. {
                    failures.push(format!(
                        "{palette:?}/UI{ui}: {name} {:?}, expected {expected}",
                        bounds.size
                    ));
                }
            }
            for (name, expected) in [
                ("popup-heading", 24. * ui / 13.),
                ("search-row-0", 26. * ui / 13.),
                ("failed-server-disc", 7. * ui / 13.),
            ] {
                let bounds = cx.debug_bounds(name).unwrap();
                if (f32::from(bounds.size.height) - expected).abs() > 1. {
                    failures.push(format!(
                        "{palette:?}/UI{ui}: {name} {:?}, expected {expected}",
                        bounds.size
                    ));
                }
            }
            root.update(cx, |root, _| {
                root.settings.terminal_font_size = 12.5;
            });
            host.update(cx, |host, cx| {
                host.page = true;
                cx.notify();
            });
            settle(cx);
            let preview = cx.debug_bounds("terminal-font-preview").unwrap();
            let expected = 3. * 12.5 * theme::leading::TERMINAL + 2. * 13. * ui / 13.;
            if (f32::from(preview.size.height) - expected).abs() > 2. {
                failures.push(format!(
                    "{palette:?}/UI{ui}: terminal preview {:?}, expected height {expected}",
                    preview.size
                ));
            }
        }
    }
    assert!(
        root.read_with(cx, |r, _| r.servers.is_empty()),
        "no real LSP"
    );
    guild_session::flush();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
