//! Isolated native harness failure acceptance using the production Root and controls.
use gpui::{App, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
fn main() -> anyhow::Result<()> {
    let state = std::env::var_os("GUILD_STATE_DIR")
        .ok_or_else(|| anyhow::anyhow!("Set task-owned GUILD_STATE_DIR"))?;
    anyhow::ensure!(
        std::fs::read_to_string(std::path::Path::new(&state).join(".owned-harness-fixture"))
            .ok()
            .as_deref()
            == Some("Guild isolated harness fixture\n"),
        "Use task-owned marked fixture state"
    );
    anyhow::ensure!(
        std::path::Path::new(&state).is_absolute()
            && std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref() == Ok("1"),
        "Use isolated absolute state and disabled global hooks"
    );
    gpui_kit::application()
        .with_assets(guild_desktop::icons::Icons)
        .run(|cx: &mut App| {
            #[cfg(feature = "native-policy-probe")]
            println!("{}", serde_json::json!({"actual_native_permissions":guild_desktop::system::window_probe::permissions(),"pid":std::process::id()}));
            guild_desktop::fonts::register(cx);
            guild_desktop::editor::init(cx);
            guild_desktop::actions::install(cx);
            guild_desktop::editor::apply_theme(&guild_desktop::theme::Theme::default(), 13., cx);
            cx.on_action(|_: &guild_desktop::actions::Quit, cx| {
                guild_session::flush();
                cx.quit();
            });
            cx.on_window_closed(|cx, _| {
                guild_session::flush();
                cx.quit();
            })
            .detach();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1160.), px(900.)),
                        cx,
                    ))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Guild Harness Failure Fixture".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let fixture = std::path::PathBuf::from(std::env::var_os("GUILD_FAILURE_FIXTURE").expect("fixture root"));
                    let root = guild_desktop::native_preview::harness_failure::root(&fixture, window, cx).expect("owned fixture");
                    cx.new(|cx| gpui_component::Root::new(root, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
    Ok(())
}
