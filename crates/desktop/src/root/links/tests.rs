//! A terminal's `cmd`-click as the window takes it: a real Root, isolated
//! state, no services, and AppKit's launcher intercepted.
use super::*;
use crate::Body;
use crate::system::default_app::tests::Capture;
use crate::terminal::links::Clicked;
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, px};
use guild_session::ProjectId;
use std::path::{Path, PathBuf};

#[test]
fn isolated_terminal_link_clicks() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::links::tests::",
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

const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"60\"/>";
const PDF: &[u8] = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj\0";

struct Fixture<'a> {
    /// The project.
    dir: tempfile::TempDir,
    /// Somewhere else on the disk, as `~/.local/share/…` is.
    outside: tempfile::TempDir,
    root: Entity<Root>,
    cx: &'a mut VisualTestContext,
    project: ProjectId,
}

fn mount(cx: &mut TestAppContext) -> Fixture<'_> {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("inside.txt"), "a\nb\nc\nd\n").unwrap();
    std::fs::write(dir.path().join("inside.pdf"), PDF).unwrap();
    std::fs::write(outside.path().join("qr.svg"), SVG).unwrap();
    std::fs::write(outside.path().join("notes.txt"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(outside.path().join("report.pdf"), PDF).unwrap();
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
        outside,
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

    /// A `cmd`-click on `path` printed in a terminal started in the project.
    fn click(&mut self, path: PathBuf, line: Option<u32>, column: Option<u32>) {
        let cwd = self.dir.path().to_path_buf();
        let root = self.dir.path().to_path_buf();
        self.cx.update(|window, cx| {
            self.root.update(cx, |r, cx| {
                r.open_link(Clicked { path, line, column }, cwd, root, window, cx)
            })
        });
        self.settle();
    }

    /// The file tabs open, by origin and path.
    fn files(&mut self) -> Vec<(crate::Origin, PathBuf)> {
        self.cx.update(|_, cx| {
            self.root
                .read(cx)
                .tabs
                .iter()
                .flat_map(crate::Tab::bodies)
                .filter_map(|body| match body {
                    Body::File { path, origin, .. } => Some((*origin, path.clone())),
                    _ => None,
                })
                .collect()
        })
    }

    fn front(&mut self) -> gpui::Entity<crate::file_tab::FileTab> {
        self.cx
            .update(|_, cx| match self.root.read(cx).active_body().unwrap() {
                Body::File { tab, .. } => tab.clone(),
                _ => panic!("a file in front"),
            })
    }

    fn notices(&mut self) -> Vec<String> {
        self.cx.update(|_, cx| {
            self.root
                .read(cx)
                .notices
                .visible()
                .map(|(_, notice)| notice.text.clone())
                .collect()
        })
    }

    fn real(&self, path: &Path) -> PathBuf {
        path.canonicalize().unwrap()
    }
}

/// The path an `Origin::Outside` tab keeps: relative to `/`.
fn under_slash(path: &Path) -> PathBuf {
    path.strip_prefix("/").unwrap().to_path_buf()
}

#[gpui::test]
#[ignore = "isolated_terminal_link_clicks owns state"]
fn a_file_outside_the_project_opens_read_only_and_is_nobodys_root(cx: &mut TestAppContext) {
    let mut f = mount(cx);
    let launches = Capture::new();
    let svg = f.real(&f.outside.path().join("qr.svg"));

    f.click(svg.clone(), None, None);

    assert_eq!(
        f.files(),
        [(crate::Origin::Outside, under_slash(&svg))],
        "a tab of its own kind, not the notice"
    );
    let file = f.front();
    f.cx.update(|_, cx| {
        let file = file.read(cx);
        assert_eq!(file.path(), svg);
        assert_eq!(
            file.image_dimensions(),
            Some((120, 60)),
            "an SVG is a preview, as inside the project: {:?}",
            file.image_error()
        );
    });
    assert!(f.notices().is_empty(), "{:?}", f.notices());
    assert!(launches.paths().is_empty(), "an image is Guild's");

    // Not a root: not in the libraries, not written to the project.
    f.cx.update(|_, cx| {
        f.root.update(cx, |root, _| {
            assert!(root.libraries.of(f.project).is_empty());
            root.store_tabs();
            // What Recent Files writes for the tab in front.
            root.note_front_file();
        })
    });
    let state = guild_session::project_state::load(f.dir.path());
    assert!(state.external_roots.is_empty());
    assert!(
        state.tabs.is_empty(),
        "a path relative to `/` is not written down as the project's: {:?}",
        state.tabs
    );
    assert!(state.recent_files.is_empty(), "{:?}", state.recent_files);

    // A second click is the way back to the same tab.
    f.click(svg.clone(), None, None);
    assert_eq!(f.files().len(), 1);
}

#[gpui::test]
#[ignore = "isolated_terminal_link_clicks owns state"]
fn an_outside_text_file_lands_on_its_line_and_is_never_written(cx: &mut TestAppContext) {
    let mut f = mount(cx);
    let _launches = Capture::new();
    let notes = f.real(&f.outside.path().join("notes.txt"));

    f.click(notes.clone(), Some(2), Some(3));

    assert_eq!(f.files(), [(crate::Origin::Outside, under_slash(&notes))]);
    let file = f.front();
    f.cx.update(|_, cx| {
        let editor = file.read(cx).editor().expect("text is in the editor");
        let at = editor.read(cx).cursor_position();
        assert_eq!((at.line, at.character), (1, 2), "`:2:3` is one-based");
    });
    let refused =
        f.cx.update(|_, cx| file.update(cx, |file, cx| file.save(cx)));
    assert!(refused.is_err(), "cmd-s never writes it");
    assert_eq!(
        std::fs::read_to_string(&notes).unwrap(),
        "one\ntwo\nthree\n"
    );
    f.cx.update(|_, cx| assert_eq!(file.read(cx).mark(), None));
}

#[gpui::test]
#[ignore = "isolated_terminal_link_clicks owns state"]
fn what_guild_cannot_show_goes_to_its_application_inside_and_outside(cx: &mut TestAppContext) {
    let mut f = mount(cx);
    let launches = Capture::new();
    let outside = f.real(&f.outside.path().join("report.pdf"));
    let inside = f.real(&f.dir.path().join("inside.pdf"));

    f.click(outside.clone(), None, None);
    f.click(PathBuf::from("inside.pdf"), None, None);

    assert_eq!(launches.paths(), [outside.clone(), inside]);
    assert!(f.files().is_empty(), "no tab of bytes it cannot show");
    assert!(f.notices().is_empty(), "{:?}", f.notices());

    // An application found and then failing to open the file says so only
    // later, through AppKit's handler; the window is waiting by then, and
    // that is a notice too.
    launches.hold();
    f.click(outside.clone(), None, None);
    let damaged = |said: &[String]| {
        said.iter()
            .any(|text| text.contains(&outside.display().to_string()) && text.contains("damaged"))
    };
    assert!(!damaged(&f.notices()), "nothing said before AppKit answers");
    launches.complete(Err("The application is damaged".into()));
    f.settle();
    let said = f.notices();
    assert!(damaged(&said), "{said:?}");

    // A failed hand-off is said, not dropped.
    launches.fail_with("No application is set to open this kind of file");
    f.click(outside.clone(), None, None);
    let said = f.notices();
    assert!(
        said.iter()
            .any(|text| text.contains(&outside.display().to_string())
                && text.contains("No application")),
        "{said:?}"
    );
}

#[gpui::test]
#[ignore = "isolated_terminal_link_clicks owns state"]
fn a_project_file_opens_as_before_and_a_program_or_a_missing_file_is_a_notice(
    cx: &mut TestAppContext,
) {
    let mut f = mount(cx);
    let launches = Capture::new();

    f.click(PathBuf::from("inside.txt"), Some(3), None);
    assert_eq!(
        f.files(),
        [(crate::Origin::Project, PathBuf::from("inside.txt"))]
    );
    let file = f.front();
    f.cx.update(|_, cx| {
        let at = file.read(cx).editor().unwrap().read(cx).cursor_position();
        assert_eq!(at.line, 2);
    });

    {
        use std::os::unix::fs::PermissionsExt as _;
        let tool = f.outside.path().join("tool");
        std::fs::write(&tool, b"\xcf\xfa\xed\xfe\x07\0\0\x01").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        f.click(tool, None, None);
        // A Finder alias, by its bookmark header: macOS would open its
        // original, which may be an application.
        let alias = f.outside.path().join("minutes.pdf");
        std::fs::write(&alias, b"book\0\0\0\0mark\0\0\0\0\x38\0\0\0").unwrap();
        f.click(alias, None, None);
    }
    f.click(PathBuf::from("gone.txt"), None, None);

    assert!(launches.paths().is_empty(), "a program is never run");
    let said = f.notices();
    assert!(
        said.iter().any(|t| t.contains("program"))
            && said.iter().any(|t| t.contains("minutes.pdf")),
        "{said:?}"
    );
    assert!(said.iter().any(|t| t.contains("gone.txt")), "{said:?}");
    assert_eq!(f.files().len(), 1);
}
