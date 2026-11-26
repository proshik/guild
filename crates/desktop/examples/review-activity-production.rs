//! Production Root/card against task-owned schema streams, never providers.
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
fn main() -> anyhow::Result<()> {
    let state = std::path::PathBuf::from(
        std::env::var_os("GUILD_STATE_DIR")
            .ok_or_else(|| anyhow::anyhow!("Set isolated GUILD_STATE_DIR"))?,
    );
    let fixture = state.parent().unwrap().join("project");
    anyhow::ensure!(
        std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref() == Ok("1")
            && fixture.join(".owned-review-fixture").is_file(),
        "Use owned fixture and disabled hooks"
    );
    gpui_kit::application()
        .with_assets(guild_desktop::icons::Icons)
        .run(move |cx: &mut App| {
            guild_desktop::fonts::register(cx);
            guild_desktop::editor::init(cx);
            guild_desktop::actions::install(cx);
            guild_desktop::editor::apply_theme(&guild_desktop::theme::Theme::default(), 13., cx);
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1100.), px(800.)),
                        cx,
                    ))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Guild · production Review activity · local fixtures".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let root =
                        guild_desktop::native_preview::review_activity::root(&fixture, window, cx)
                            .unwrap();
                    cx.new(|cx| gpui_component::Root::new(root, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
    Ok(())
}
