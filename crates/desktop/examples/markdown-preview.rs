//! Isolated Markdown pane for native checks; no projects, sessions or persistent state.
use gpui::prelude::*;
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use guild_desktop::{
    file_tab::{FileTab, Look},
    i18n::Ui,
    markdown::Presentation,
    theme::Theme,
};
fn main() -> anyhow::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Provide a Markdown file"))?;
    let path = std::fs::canonicalize(path)?;
    let theme = if std::env::args().any(|arg| arg == "--light") {
        Theme::of(guild_desktop::theme::Kind::Light)
    } else {
        Theme::default()
    };
    gpui_kit::application()
        .with_assets(guild_desktop::icons::Icons)
        .run(move |cx: &mut App| {
            guild_desktop::fonts::register(cx);
            guild_desktop::editor::init(cx);
            guild_desktop::editor::apply_theme(&theme, 14., cx);
            guild_desktop::menu::install(Ui(guild_session::Language::English), &[], cx);
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(900.), px(700.)),
                        cx,
                    ))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Guild Markdown — isolated dev preview".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let file = cx.new(|cx| {
                        FileTab::open(
                            path.parent().unwrap(),
                            std::path::Path::new(path.file_name().unwrap()),
                            Look {
                                family: "monospace".into(),
                                font_size: 14.,
                                theme,
                                autosave: false,
                            },
                            Ui(guild_session::Language::English),
                            window,
                            cx,
                        )
                    });
                    let pane =
                        Presentation::for_file(&file, guild_session::markdown::Mode::Split, cx)
                            .expect("Markdown file");
                    let host = cx.new(|_| Preview(pane, theme));
                    cx.new(|cx| gpui_component::Root::new(host, window, cx))
                },
            )
            .expect("open isolated Markdown window");
            cx.activate(true);
        });
    Ok(())
}

struct Preview(Presentation, Theme);
impl Render for Preview {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        gpui::div()
            .size_full()
            .bg(gpui::rgb(self.1.surface.app))
            .text_color(gpui::rgb(self.1.text.primary))
            .flex()
            .flex_col()
            .child(self.0.controls.clone())
            .child(self.0.view.clone())
    }
}
