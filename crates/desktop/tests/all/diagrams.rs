use guild_desktop::diagrams::{Engine, Resources};
#[test]
fn cancellation_and_deadline_stop_a_helper_that_never_answers() {
    use std::{
        os::unix::fs::PermissionsExt,
        sync::atomic::AtomicBool,
        time::{Duration, Instant},
    };
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper");
    std::fs::write(&helper, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let error = guild_desktop::diagrams::transport::render(
        &helper,
        dir.path(),
        "mermaid",
        "flowchart LR\n A-->B",
        &cancel,
        Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(5));
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            cancel.store(true, std::sync::atomic::Ordering::Release);
        });
        let error = guild_desktop::diagrams::transport::render(
            &helper,
            dir.path(),
            "mermaid",
            "A",
            &cancel,
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
    });
}

struct HelperImage(std::path::PathBuf);
impl gpui::Render for HelperImage {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        gpui::div().child(gpui::img(self.0.clone()).debug_selector(|| "helper-png".into()))
    }
}
#[gpui::test]
#[ignore = "requires python3 tools/diagrams/renderer/test_helper.py"]
fn webkit_png_decodes_physical_data_without_setting_preview_geometry(
    cx: &mut gpui::TestAppContext,
) {
    for name in ["mermaid", "plantuml"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../target/diagram-runtime-results/{name}.png"));
        let bytes = std::fs::read(&path).unwrap();
        let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
        let (_, cx) = cx.add_window_view(|_, _| HelperImage(path));
        cx.run_until_parked();
        cx.simulate_resize(gpui::size(gpui::px(1000.), gpui::px(800.)));
        let bounds = cx
            .debug_bounds("helper-png")
            .expect("raw PNG decode preserves physical dimensions; preview layout has separate logical metadata");
        assert_eq!(
            bounds.size,
            gpui::size(gpui::px(width as f32), gpui::px(height as f32))
        );
    }
}

#[gpui::test]
#[ignore = "requires explicit prepared bundled resources"]
fn bundled_diagrams_render_with_empty_installed_state_without_installation(
    cx: &mut gpui::TestAppContext,
) {
    use gpui::AppContext;
    use guild_desktop::diagrams::runtime::{DiagramView, Manager};
    let empty = tempfile::tempdir().unwrap();
    let resources = fixture_resources();
    assert!(
        resources.helper.is_file(),
        "prepare the actual fixture first"
    );
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let manager = cx.new(|cx| Manager::new(Ok(resources), cx));
        let view = DiagramView::new(
            manager,
            Engine::Mermaid,
            "flowchart LR\n A[\"Привет\"]-->B",
            cx,
        );
        gpui_component::Root::new(view, window, cx)
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("diagram-install-helper").is_none(),
        "shipped preview must not offer installation"
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(20));
        cx.run_until_parked();
        if cx.debug_bounds("diagram-image-mermaid").is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "bundled resources must render immediately without installation"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        std::fs::read_dir(empty.path()).unwrap().count(),
        0,
        "no installed state needed or written"
    );
}

pub(super) fn fixture_resources() -> Resources {
    if let Some(app) = std::env::var_os("GUILD_DIAGRAM_TEST_APP") {
        Resources::for_executable(
            &std::path::PathBuf::from(app).join("Contents/MacOS/guild-desktop"),
            None,
        )
        .unwrap()
    } else {
        Resources::from_directory(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/diagram-runtime"),
        )
    }
}
fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target)
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
#[test]
fn app_resource_selection_ignores_development_override_and_external_state() {
    let temp = tempfile::tempdir().unwrap();
    let app = temp.path().join("Приложения с пробелом/Guild.app");
    let resources =
        Resources::for_executable(&app.join("Contents/MacOS/guild-desktop"), Some(temp.path()))
            .unwrap();
    assert_eq!(resources.assets, app.join("Contents/Resources/diagrams"));
    assert_eq!(
        resources.helper,
        app.join("Contents/MacOS/guild-diagram-renderer")
    );
    let error = resources.verify(Engine::Mermaid).unwrap_err();
    assert!(format!("{error:#}").contains("Reinstall Guild"));
    assert_eq!(
        std::fs::read_dir(temp.path()).unwrap().count(),
        0,
        "discovery must not create or adopt state"
    );
}
#[test]
#[ignore = "requires explicit prepared bundled resources"]
fn bundled_integrity_refuses_missing_or_corrupt_resources_before_execution() {
    let temp = tempfile::tempdir().unwrap();
    let origin = fixture_resources();
    copy_tree(&origin.assets, temp.path());
    std::fs::copy(origin.helper, temp.path().join("guild-diagram-renderer")).unwrap();
    let resources = Resources::from_directory(temp.path());
    resources.verify(Engine::Mermaid).unwrap();
    resources.verify(Engine::PlantUml).unwrap();
    let js = temp.path().join("mermaid-11.12.0/mermaid.min.js");
    std::fs::write(&js, b"corrupt").unwrap();
    let error = format!("{:#}", resources.verify(Engine::Mermaid).unwrap_err());
    assert!(error.contains("integrity mismatch") && error.contains("Reinstall Guild"));
    std::fs::remove_file(js).unwrap();
    assert!(resources.verify(Engine::Mermaid).is_err());
    std::fs::write(&resources.helper, b"corrupt executable").unwrap();
    assert!(resources.verify(Engine::PlantUml).is_err());
}
#[test]
#[ignore = "requires GUILD_DIAGRAM_TEST_APP naming an actual assembled app"]
fn moved_bundled_app_renders_both_engines_offline_without_installed_state() {
    let temp = tempfile::tempdir().unwrap();
    let original = std::path::PathBuf::from(
        std::env::var_os("GUILD_DIAGRAM_TEST_APP").expect("build the reviewable app first"),
    );
    let app = temp.path().join("Перенесённое приложение/Guild copy.app");
    copy_tree(&original, &app);
    assert!(
        std::process::Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&app)
            .status()
            .unwrap()
            .success()
    );
    // Reopen resources through the moved main executable's path, with a hostile
    // external override that a production bundle must never adopt.
    let resources = Resources::for_executable(
        &app.join("Contents/MacOS/guild-desktop"),
        Some(&temp.path().join("old components")),
    )
    .unwrap();
    for (engine, source) in [
        (Engine::Mermaid, "flowchart TD\n A[\"Привет\"]-->B"),
        (Engine::PlantUml, "@startuml\nAlice -> Bob: Привет\n@enduml"),
    ] {
        resources.verify(engine).unwrap();
        let reply = guild_desktop::diagrams::transport::render(
            &resources.helper,
            &resources.assets,
            engine.id(),
            source,
            &std::sync::atomic::AtomicBool::new(false),
            std::time::Duration::from_secs(15),
        )
        .unwrap();
        assert!(reply.get("png").is_some(), "{reply}");
    }
    assert!(!temp.path().join("old components").exists());
}
