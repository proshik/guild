//! Native owned-bundle updater acceptance, never the user's installed app.
use gpui::{App, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use guild_session::update::{bundle, transaction::Startup};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if let Some(result) = guild_desktop::update::helper::entry(&args) {
        return result.map_err(|failure| anyhow::anyhow!("fixture helper stopped: {failure:?}"));
    }
    anyhow::ensure!(args.is_empty(), "fixture accepts no public commands");
    let state = std::env::var_os("GUILD_STATE_DIR")
        .ok_or_else(|| anyhow::anyhow!("set owned fixture state"))?;
    let state = std::path::PathBuf::from(state).canonicalize()?;
    anyhow::ensure!(
        bundle::read_small(&state.join(".owned-updater-fixture"), 128)
            .map_err(anyhow::Error::msg)?
            .as_slice()
            == b"Guild isolated updater fixture\n",
        "use only task-owned updater state"
    );
    let executable = std::env::current_exe()?.canonicalize()?;
    let app = guild_desktop::update::running_bundle(&executable)
        .ok_or_else(|| anyhow::anyhow!("fixture must run inside its owned app"))?;
    anyhow::ensure!(
        app == state.join("apps/Guild.app"),
        "fixture app must be under owned state/apps"
    );
    let info = bundle::info(&app).map_err(anyhow::Error::msg)?;
    let instance = uuid::Uuid::new_v4();
    bundle::publish_json(
        &state.join(format!("process-{instance}.json")),
        &serde_json::json!({
            "instance":instance,"pid":std::process::id(),"build":info,
            "permissions":guild_desktop::system::window_probe::permissions()
        }),
    )
    .map_err(anyhow::Error::msg)?;
    let expected: uuid::Uuid = String::from_utf8(
        bundle::read_small(&state.join("expected-build"), 64).map_err(anyhow::Error::msg)?,
    )?
    .trim()
    .parse()?;
    if state.join("fail-new-startup").exists() && info.build_id == expected {
        std::process::exit(42);
    }
    let startup = Startup::capture(state.clone()).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let approval = if state.join("approve-once").exists() {
        std::fs::remove_file(state.join("approve-once"))?;
        Some(expected)
    } else {
        None
    };
    gpui_kit::application()
        .with_assets(guild_desktop::icons::Icons)
        .run(move |cx: &mut App| {
            guild_desktop::fonts::register(cx);
            guild_desktop::editor::init(cx);
            guild_desktop::actions::install(cx);
            guild_desktop::editor::apply_theme(&guild_desktop::theme::Theme::default(), 13., cx);
            guild_desktop::system::install_should_terminate();
            cx.on_window_closed(|cx, _| guild_desktop::quit(cx))
                .detach();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1180.), px(760.)),
                        cx,
                    ))),
                    app_owns_titlebar_drag: true,
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Guild Updater Acceptance".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let preview = cx.new(|cx| {
                        guild_desktop::native_preview::updater::UpdaterPreview::new(
                            state, info, startup, approval, instance, window, cx,
                        )
                    });
                    cx.new(|cx| gpui_component::Root::new(preview, window, cx))
                },
            )
            .expect("owned updater fixture window");
            cx.activate(true);
        });
    Ok(())
}
