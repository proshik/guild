//! Task-owned native trial of Git row tints and identifier occurrences.
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
    let fixture = std::path::Path::new(&state).join("fixture");
    anyhow::ensure!(
        fixture.join(".owned-tooltip-fixture").is_file(),
        "owned fixture required"
    );
    let snapshot = guild_desktop::native_preview::tree_occurrences::Snapshot::read(&fixture)?;
    let background = std::env::var("GUILD_PREVIEW_BACKGROUND").as_deref() == Ok("1");
    let annotate = (std::env::var("GUILD_TREE_SCENE").as_deref() == Ok("annotate"))
        .then(|| std::fs::read_to_string(fixture.join("dense.js")))
        .transpose()?;
    gpui_kit::application()
        .with_assets(guild_desktop::icons::Icons)
        .run(move |cx: &mut App| {
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
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.open_window(
                WindowOptions {
                    focus: !background,
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1160.), px(900.)),
                        cx,
                    ))),
                    app_owns_titlebar_drag: true,
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Guild Tree Occurrences Trial".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(gpui::point(
                            px(12.0),
                            px(guild_desktop::theme::space::TRAFFIC_LIGHTS_TOP),
                        )),
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    if let Some(source) = annotate {
                        let tab = cx.new(|cx| {
                            guild_desktop::annotate_tab::AnnotateTab::new(
                                "dense.js".into(),
                                guild_desktop::file_tab::Look {
                                    family: "Menlo".into(),
                                    font_size: 13.,
                                    theme: Default::default(),
                                    autosave: false,
                                },
                                guild_desktop::i18n::Ui(guild_session::Language::English),
                                cx,
                            )
                        });
                        tab.update(cx, |tab, cx| {
                            tab.set_runs(
                                Ok(vec![guild_desktop::git::blame::Run {
                                    sha: "a".repeat(40),
                                    author: "Fixture".into(),
                                    time: 1,
                                    summary: "Dense source preparation".into(),
                                    first_line: 1,
                                    original_line: 1,
                                    filename: "dense.js".into(),
                                    lines: vec![source],
                                }]),
                                cx,
                            )
                        });
                        return cx.new(|cx| gpui_component::Root::new(tab, window, cx));
                    }
                    let fixture = guild_desktop::paths::state_dir().join("fixture");
                    let root = guild_desktop::native_preview::tree_occurrences::root(
                        &fixture, snapshot, window, cx,
                    )
                    .expect("owned tooltip fixture");
                    cx.new(|cx| gpui_component::Root::new(root, window, cx))
                },
            )
            .unwrap();
            if !background {
                cx.activate(true);
            }
        });
    Ok(())
}
