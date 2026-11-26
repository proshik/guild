//! Development-only generic native acceptance tools.
pub mod adaptive_refresh_probe;
pub mod forge_frames;
pub mod frame_meter;
pub mod frame_navigation;
pub mod harness_failure;
pub mod resource_probe;
pub mod updater;

/// The production no-project Settings host, with external integrations disabled.
pub fn network_root(_window: &mut gpui::Window, cx: &mut gpui::App) -> gpui::Entity<crate::Root> {
    use gpui::AppContext;
    let settings =
        guild_session::Store::<guild_session::Settings>::new(crate::paths::settings_file())
            .load()
            .unwrap_or_default();
    let state = crate::paths::state_dir();
    cx.new(|cx| {
        let mut root = crate::Root::from_state_with_policy(
            settings,
            Default::default(),
            crate::root::isolation::Policy::Diagnostic { state_dir: state },
            cx,
        );
        root.open_settings_at(crate::view::settings::Section::Network, cx);
        root
    })
}

pub mod review_activity;
/// Task-owned control trial using the production frame without external services.
pub fn tooltip_root(
    fixture: &std::path::Path,
    _window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> anyhow::Result<gpui::Entity<crate::Root>> {
    use gpui::AppContext;
    anyhow::ensure!(
        fixture.is_absolute()
            && fixture.starts_with(crate::paths::state_dir())
            && fixture.join(".owned-tooltip-fixture").is_file(),
        "Use the marked task-owned tooltip fixture"
    );
    let mut desk = guild_session::Desk::default();
    let project = desk.open_in(desk.active_workspace(), fixture);
    desk.look_at(Some(project), None);
    let settings =
        guild_session::Store::<guild_session::Settings>::new(crate::paths::settings_file())
            .load()
            .unwrap_or_default();
    let root = cx.new(|cx| {
        crate::Root::from_state_with_policy(
            settings,
            desk,
            crate::root::isolation::Policy::Diagnostic {
                state_dir: crate::paths::state_dir(),
            },
            cx,
        )
    });
    root.update(cx, |root, cx| {
        root.settings.default_tab = guild_session::DefaultTab::Terminal;
        root.ensure_open_in_apps();
        eprintln!(
            "Open In preview: selected={:?}; apps={:?}; icon_ids={:?}",
            root.selected_open_in_target(),
            root.open_in_apps,
            root.open_in_icons.borrow().keys().collect::<Vec<_>>()
        );
        root.ensure_active_tree(cx);
        cx.notify();
    });
    Ok(root)
}

pub mod runtime_diff;
pub mod runtime_idle;
pub mod runtime_worktrees;

/// Scale audit on an owned fixture; never starts a provider, shell or language server.
pub fn scale_audit_root(
    fixture: &std::path::Path,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> anyhow::Result<gpui::Entity<crate::Root>> {
    let root = tooltip_root(fixture, window, cx)?;
    root.update(cx, |root, cx| {
        let project = root.active_project.expect("fixture project");
        root.open_file(std::path::Path::new("example.rs"), window, cx);
        root.scoped_settings.identities.insert(
            project,
            Ok(crate::repository_settings::RepositoryIdentity {
                key: fixture.into(),
                main: Some(fixture.into()),
                git: false,
            }),
        );
        root.serverless
            .insert((project, "rust"), crate::lsp::progress::Fault::Missing);
        root.reveal_tool(guild_session::Tool::Explorer, cx);
        root.ensure_search(window, cx);
        root.searches.get_mut(&project).unwrap().query.text = "needle".into();
        root.restart_search(project, cx);
        root.reveal_tool(guild_session::Tool::Search, cx);
        root.with_panels(|panels| {
            panels.bottom.size = 280.;
            panels.left.size = 300.;
        });
        if std::env::var("GUILD_SCALE_SCENE").as_deref() == Ok("terminal") {
            root.hide_dock(guild_session::Dock::Left, cx);
            root.hide_dock(guild_session::Dock::Bottom, cx);
            root.open_settings_at(crate::view::settings::Section::Terminal, cx);
        }
        cx.notify();
    });
    // Diagnostic policy deliberately skips the integration poll. Poll only the
    // fixture's local search, retaining the normal publication and navigation path.
    let weak = root.downgrade();
    window
        .spawn(cx, async move |cx| {
            loop {
                if !matches!(
                    cx.update(|_, cx| weak.update(cx, |root, cx| root.poll_search(cx)).is_ok()),
                    Ok(true)
                ) {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
            }
        })
        .detach();
    Ok(root)
}

pub mod tree_occurrences;
