//! Isolated native control trial using the production Root and controls.
use gpui::{App, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
fn main() -> anyhow::Result<()> {
    let state = std::env::var_os("GUILD_STATE_DIR")
        .ok_or_else(|| anyhow::anyhow!("Set task-owned GUILD_STATE_DIR"))?;
    anyhow::ensure!(
        std::fs::read_to_string(std::path::Path::new(&state).join(".owned-tooltip-preview"))
            .ok()
            .as_deref()
            == Some("Guild isolated tooltip preview\n"),
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
                    app_owns_titlebar_drag: true,
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Guild UI Tooltip Trial".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(gpui::point(
                            px(12.0),
                            px(guild_desktop::theme::space::TRAFFIC_LIGHTS_TOP),
                        )),
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let fixture = guild_desktop::paths::state_dir().join("fixture");
                    let root = guild_desktop::native_preview::tooltip_root(&fixture, window, cx)
                        .expect("owned tooltip fixture");
                    cx.new(|cx| gpui_component::Root::new(root, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
    Ok(())
}
