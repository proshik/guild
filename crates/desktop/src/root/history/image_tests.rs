//! Real Root navigation and key dispatch, with isolated state and no services.
use super::*;
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, px};
use std::path::Path;

#[test]
fn isolated_image_navigation_and_keys() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::history::image_tests::",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
struct Fixture<'a> {
    dir: tempfile::TempDir,
    root: Entity<Root>,
    cx: &'a mut VisualTestContext,
    project: ProjectId,
}
const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"60\">\n<text>Привет needle</text>\n</svg>";
fn mount(cx: &mut TestAppContext) -> Fixture<'_> {
    let dir = tempfile::tempdir().unwrap();
    for name in ["hit.svg", "protocol.svg"] {
        std::fs::write(dir.path().join(name), SVG).unwrap();
    }
    let mut made = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| {
            let mut root = Root::from_state(
                guild_session::Settings {
                    autosave: false,
                    ..Default::default()
                },
                desk,
                cx,
            );
            root.active_project = Some(project);
            root.isolated_render = true;
            root.restored_workspaces.insert(root.active_workspace);
            root
        });
        made = Some((root.clone(), project));
        gpui_component::Root::new(root, window, cx)
    });
    visual.simulate_resize(gpui::size(px(1200.), px(800.)));
    visual.run_until_parked();
    let (root, project) = made.unwrap();
    Fixture {
        dir,
        root,
        cx: visual,
        project,
    }
}
impl Fixture<'_> {
    fn settle(&mut self) {
        self.cx.run_until_parked();
        for _ in 0..10 {
            self.cx.update(|window, cx| {
                window.simulate_next_frame(cx);
                window.refresh();
            });
            self.cx.run_until_parked();
        }
    }
    fn file(&mut self) -> gpui::Entity<crate::file_tab::FileTab> {
        self.cx
            .update(|_, cx| match self.root.read(cx).active_body().unwrap() {
                Body::File { tab, .. } => tab.clone(),
                _ => panic!("file"),
            })
    }
    fn click(&mut self, id: &'static str) {
        let bounds = self.cx.debug_bounds(id).unwrap();
        self.cx.simulate_click(bounds.center(), Default::default());
        self.settle();
    }
}
#[gpui::test]
#[ignore = "isolated_image_navigation_and_keys owns state"]
fn unopened_svg_search_and_protocol_destinations_survive_loading(cx: &mut TestAppContext) {
    let mut f = mount(cx);
    f.cx.update(|window, cx| {
        f.root.update(cx, |r, cx| {
            r.open_file_at("hit.svg".into(), 2, 13, window, cx);
            r.select_on_landing(f.project, Place::new("hit.svg", 1, 13), 6, window, cx);
            assert!(
                r.history.landing.is_some(),
                "unopened SVG source destination must survive asynchronous loading"
            );
        })
    });
    // More than the old five-frame budget can elapse before background work.
    for _ in 0..8 {
        f.cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
    }
    f.settle();
    let file = f.file();
    f.cx.update(|_, cx| {
        let file = file.read(cx);
        assert!(file.image_source_visible());
        let editor = file.editor().unwrap().read(cx);
        assert_eq!(editor.cursor_position().line, 1);
        assert_eq!(editor.cursor_position().character, 19);
        assert_eq!(
            editor.selected_range().len(),
            6,
            "the search match must be selected"
        );
    });
    f.cx.update(|window, cx| {
        f.root.update(cx, |r, cx| {
            r.land_at(
                f.project,
                "protocol.svg".into(),
                lsp_types::Position::new(1, 13),
                window,
                cx,
            );
            assert!(r.landing.is_some(), "protocol destination must wait too");
        })
    });
    f.settle();
    let file = f.file();
    f.cx.update(|_, cx| {
        let file = file.read(cx);
        assert!(file.image_source_visible());
        let at = file.editor().unwrap().read(cx).cursor_position();
        assert_eq!((at.line, at.character), (1, 13));
    });
    f.cx.update(|window, cx| {
        f.root.update(cx, |r, cx| {
            r.land_at(
                f.project,
                "gone.svg".into(),
                lsp_types::Position::new(1, 13),
                window,
                cx,
            )
        })
    });
    f.settle();
    f.cx.update(|_, cx| {
        assert!(
            f.root.read(cx).landing.is_none(),
            "definitive failure releases the destination"
        )
    });
}
#[gpui::test]
#[ignore = "isolated_image_navigation_and_keys owns state"]
fn preview_keyboard_save_and_close_reach_the_owning_root(cx: &mut TestAppContext) {
    let mut f = mount(cx);
    f.cx.update(|window, cx| {
        f.root
            .update(cx, |r, cx| r.open_file(Path::new("hit.svg"), window, cx))
    });
    f.settle();
    let file = f.file();
    f.cx.update(|window, cx| {
        use gpui::Focusable;
        assert!(
            file.read(cx).focus_handle(cx).is_focused(window),
            "asynchronous realization must retain Preview focus"
        );
        assert!(
            !file
                .read(cx)
                .editor()
                .unwrap()
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
    f.click("image-source");
    f.cx.simulate_keystrokes("cmd-a");
    let edited = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"12\"/>";
    f.cx.simulate_input(edited);
    f.click("image-source");
    f.cx.simulate_keystrokes("cmd-s");
    f.settle();
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("hit.svg")).unwrap(),
        edited,
        "Save must reach Root from Preview"
    );
    f.cx.simulate_keystrokes("cmd-w");
    f.settle();
    f.cx.update(|_, cx| {
        assert!(
            !f.root
                .read(cx)
                .tabs
                .iter()
                .flat_map(crate::Tab::bodies)
                .any(|b| matches!(b, Body::File { .. })),
            "Close must reach Root from Preview"
        )
    });
    f.cx.update(|window, cx| {
        f.root.update(cx, |r, cx| {
            r.open_file(Path::new("protocol.svg"), window, cx)
        })
    });
    f.settle();
    f.cx.simulate_keystrokes("cmd-w");
    f.settle();
    f.cx.update(|_, cx| {
        assert!(
            !f.root
                .read(cx)
                .tabs
                .iter()
                .flat_map(crate::Tab::bodies)
                .any(|b| matches!(b, Body::File { .. })),
            "a newly realized Preview retains working Close dispatch"
        )
    });
}

#[gpui::test]
#[ignore = "isolated_image_navigation_and_keys owns state"]
fn unopened_library_svg_retains_its_protocol_destination(cx: &mut TestAppContext) {
    let mut f = mount(cx);
    let library = tempfile::tempdir().unwrap();
    std::fs::write(library.path().join("external.svg"), SVG).unwrap();
    f.cx.update(|window, cx| {
        f.root.update(cx, |r, cx| {
            let origin = r.note_library(f.project, library.path());
            r.go_to_library(
                origin,
                Path::new("external.svg"),
                Some(Caret::Protocol(lsp_types::Position::new(1, 13))),
                window,
                cx,
            );
        })
    });
    f.settle();
    let file = f.file();
    f.cx.update(|_, cx| {
        let file = file.read(cx);
        assert!(file.image_source_visible());
        let at = file.editor().unwrap().read(cx).cursor_position();
        assert_eq!((at.line, at.character), (1, 13));
    });
}
