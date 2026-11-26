//! Isolated resource acceptance fixture; never starts providers.
use gpui::{App, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::current_dir()? == std::path::Path::new("/"),
        "Run from /"
    );
    let state = std::env::var_os("GUILD_STATE_DIR")
        .ok_or_else(|| anyhow::anyhow!("Set fresh isolated state"))?;
    let state_path = std::path::Path::new(&state);
    anyhow::ensure!(state_path.is_absolute(), "Use absolute task-owned state");
    let settings_trial = std::env::var("GUILD_PROBE_SCENARIO").as_deref() == Ok("settings")
        && std::env::var("GUILD_PROBE_ROOT").as_deref() == Ok("1");
    let marker = state_path.join(".owned-settings-fixture");
    if state_path.exists() {
        anyhow::ensure!(
            settings_trial
                && std::env::var("GUILD_PROBE_SETTINGS_RESTART").as_deref() == Ok("1")
                && std::fs::read_to_string(&marker).ok().as_deref()
                    == Some("Guild isolated settings fixture\n"),
            "Use fresh state or explicitly restart the owned settings fixture"
        );
    } else if settings_trial {
        std::fs::create_dir_all(state_path)?;
        std::fs::write(marker, "Guild isolated settings fixture\n")?;
    }
    anyhow::ensure!(
        std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref() == Ok("1"),
        "Disable global hooks"
    );
    gpui_kit::application()
        .with_assets(guild_desktop::icons::Icons)
        .run(|cx: &mut App| {
            #[cfg(feature = "native-policy-probe")]
            println!("{}", serde_json::json!({"permission_preflight":guild_desktop::system::window_probe::permissions(),"pid":std::process::id()}));
            guild_desktop::fonts::register(cx);
            guild_desktop::editor::init(cx);
            guild_desktop::actions::install(cx);
            guild_desktop::editor::apply_theme(&guild_desktop::theme::Theme::default(), 13., cx);
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            let display=std::env::var("GUILD_PROBE_DISPLAY").ok().and_then(|s|s.parse::<u64>().ok()).map(gpui::DisplayId::new);
            cx.open_window(
                WindowOptions {
                    display_id: display,
                    inactive_frame_interval: if std::env::var("GUILD_PROBE_AA").as_deref()==Ok("1") && std::env::var("GUILD_PROBE_ROOT").as_deref()!=Ok("1") {None} else {WindowOptions::default().inactive_frame_interval},
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        display,
                        size(px(if std::env::var("GUILD_PROBE_ROOT").as_deref()==Ok("1") {1200.} else {900.}), px(800.)),
                        cx,
                    ))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Guild isolated resource probe".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    if std::env::var("GUILD_PROBE_ROOT").as_deref() == Ok("1") {
                        let state =
                            std::path::PathBuf::from(std::env::var_os("GUILD_STATE_DIR").unwrap());
                        let root = guild_desktop::native_preview::adaptive_refresh_probe::root(
                            &state.join(format!("fixture-{}", std::process::id())),
                            window,
                            cx,
                        )
                        .expect("isolated Root fixture");
                        return cx.new(|cx| gpui_component::Root::new(root, window, cx));
                    }
                    let view = cx.new(|cx| {
                        guild_desktop::native_preview::resource_probe::ResourceProbe::new(
                            window, cx,
                        )
                    });
                    cx.new(|cx| gpui_component::Root::new(view, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
    Ok(())
}
