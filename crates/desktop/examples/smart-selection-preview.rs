//! Isolated native acceptance surface for source extend/shrink selection.
//! Uses a disposable source and production FileTab; never opens user state.
use gpui::{App, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use guild_desktop::{
    file_tab::{FileTab, Look},
    i18n::Ui,
    theme::Theme,
};

fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var_os("GUILD_STATE_DIR").is_some_and(|p| std::path::Path::new(&p).is_absolute())
            && std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref() == Ok("1"),
        "Use absolute task-owned GUILD_STATE_DIR and GUILD_NO_GLOBAL_HOOKS=1"
    );
    let fixture = tempfile::tempdir()?;
    std::fs::write(
        fixture.path().join("main.rs"),
        "fn main() {\n    call(\"hello мир\");\n}\n\nfn call(message: &str) {\n    println!(\"{message}\");\n}\n",
    )?;
    let root = fixture.path().to_owned();
    gpui_kit::application()
        .with_assets(guild_desktop::icons::Icons)
        .run(move |cx: &mut App| {
            println!(
                "native_permissions={:?}",
                guild_desktop::system::window_probe::permissions()
            );
            guild_desktop::fonts::register(cx);
            guild_desktop::editor::init(cx);
            guild_desktop::actions::install(cx);
            guild_desktop::editor::apply_theme(&Theme::default(), 13., cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(760.), px(440.)),
                        cx,
                    ))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Guild semantic selection — isolated trial".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    let file = cx.new(|cx| {
                        FileTab::open(
                            &root,
                            std::path::Path::new("main.rs"),
                            Look {
                                family: "Menlo".into(),
                                font_size: 14.,
                                theme: Theme::default(),
                                autosave: false,
                            },
                            Ui(guild_session::Language::English),
                            window,
                            cx,
                        )
                    });
                    cx.new(|cx| gpui_component::Root::new(file, window, cx))
                },
            )
            .expect("isolated selection window");
            cx.activate(true);
        });
    Ok(())
}
