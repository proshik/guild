//! Isolated native control trial using the production Root and controls.
use gpui::{App, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
struct ScaleWindow {
    root: gpui::Entity<guild_desktop::Root>,
    terminal: Option<gpui::Entity<guild_desktop::terminal::view::TerminalTab>>,
}
impl gpui::Render for ScaleWindow {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        gpui::div()
            .size_full()
            .flex()
            .flex_col()
            .child(gpui::div().flex_1().min_h_0().child(self.root.clone()))
            .children(
                self.terminal
                    .clone()
                    .map(|terminal| gpui::div().h(px(100.)).flex_none().child(terminal)),
            )
    }
}
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
                        title: Some("Guild Scale Audit".into()),
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
                    let root =
                        guild_desktop::native_preview::scale_audit_root(&fixture, window, cx)
                            .expect("owned tooltip fixture");
                    let terminal = if std::env::var("GUILD_SCALE_PHYSICAL").as_deref() == Ok("1") {
                        let settings = guild_session::Store::<guild_session::Settings>::new(
                            guild_desktop::paths::settings_file(),
                        )
                        .load()
                        .unwrap_or_default();
                        let theme = guild_desktop::theme::in_effect(&settings, true);
                        let mut command = portable_pty::CommandBuilder::new("/usr/bin/printf");
                        command.arg(
                            "Physical terminal 12.5 px\n~/work/guild on main\nПривет, Guild\n",
                        );
                        Some(
                            guild_desktop::terminal::view::open_command(
                                command,
                                &fixture,
                                guild_desktop::terminal::view::Look::of(&settings, theme),
                                None,
                                cx,
                            )
                            .unwrap(),
                        )
                    } else {
                        None
                    };
                    let host = cx.new(|_| ScaleWindow { root, terminal });
                    cx.new(|cx| gpui_component::Root::new(host, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
    Ok(())
}
