//! Isolated notification fixture: run from / with a dedicated GUILD_STATE_DIR.
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::current_dir()? == std::path::Path::new("/"),
        "Run from / to avoid opening a project"
    );
    let state = std::env::var_os("GUILD_STATE_DIR")
        .ok_or_else(|| anyhow::anyhow!("Set isolated GUILD_STATE_DIR"))?;
    anyhow::ensure!(
        !std::path::Path::new(&state).join("workspace.json").exists(),
        "Use fresh isolated state"
    );
    gpui_kit::application()
        .with_assets(guild_desktop::icons::Icons)
        .run(|cx: &mut App| {
            guild_desktop::fonts::register(cx);
            guild_desktop::editor::init(cx);
            guild_desktop::editor::apply_theme(&guild_desktop::theme::Theme::default(), 13., cx);
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(640.), px(480.)),
                        cx,
                    ))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Guild Notice Preview".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view =
                        cx.new(|cx| guild_desktop::notice_preview::NoticePreview::new(window, cx));
                    cx.new(|cx| gpui_component::Root::new(view, window, cx))
                },
            )
            .expect("open isolated notice window");
            cx.activate(true);
        });
    Ok(())
}
