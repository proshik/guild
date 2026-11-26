//! The line numbers' half of the right-button menu — M12.9.
//!
//! Annotate toggles the target editable FileTab. The component keeps its
//! ordinary text menu, while Guild owns the measured leading gutter.

use std::path::PathBuf;

use gpui::{Context, Window};

use super::{Choice, Deed, Entry, Label, Mark, Target};
use crate::Root;
use crate::i18n::Text;

/// What the gutter's menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GutterDeed {
    /// Toggle attribution in the target file.
    Annotate,
    RunFile,
}

/// Annotation state and the file-level run action.
pub fn gutter(annotated: bool, runnable: bool) -> Vec<Entry> {
    let mut entries = vec![Entry::Choice(Choice {
        label: Label::Text(Text::MenuAnnotateWithBlame),
        mark: Mark::None,
        deed: Deed::Gutter(GutterDeed::Annotate),
        check: Some(annotated),
        enabled: true,
        detail: None,
        trailing: None,
    })];
    if runnable {
        entries.push(Entry::Item(
            Text::RunStart,
            Deed::Gutter(GutterDeed::RunFile),
        ));
    }
    entries
}

impl Root {
    /// Record a right-button press on a file's line numbers as the menu's
    /// target.
    pub(crate) fn note_gutter_menu(
        &mut self,
        path: PathBuf,
        file: gpui::Entity<crate::file_tab::FileTab>,
        cx: &gpui::App,
    ) {
        let annotated = file.read(cx).blame_enabled();
        self.note_menu_target(Target::Gutter {
            path,
            annotated,
            file,
        });
    }

    /// Carry out what the gutter's menu offers.
    pub(super) fn run_gutter_deed(
        &mut self,
        deed: GutterDeed,
        target: &Target,
        _window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Target::Gutter { file, .. } = target else {
            return;
        };
        match deed {
            GutterDeed::RunFile => {
                if file.read(cx).can_run_file() {
                    file.update(cx, |file, cx| file.run_file(cx));
                } else {
                    self.fail(self.ui().text(Text::RunSourceChanged).to_owned());
                }
            }
            GutterDeed::Annotate => file.update(cx, |file, cx| file.toggle_blame(cx)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};
    use std::path::Path;

    #[test]
    fn isolated_gutter_menu_regressions() {
        let state = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "view::menu::gutter::tests::",
                "--ignored",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[gpui::test]
    #[ignore = "runs in isolated_gutter_menu_regressions"]
    fn menu_uses_the_captured_file_and_rechecks_discovery_at_click(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("renamed.txt"), "plain text\n").unwrap();
        std::fs::write(dir.path().join("justfile"), "greet:\n    echo hello\n").unwrap();
        let mut made = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let project = desk.open_in(desk.active_workspace(), dir.path());
            let root = cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.active_project = Some(project);
                root.isolated_render = true;
                root
            });
            let file = |path: &str, cx: &mut gpui::App, window: &mut Window| {
                cx.new(|cx| {
                    crate::file_tab::FileTab::open(
                        dir.path(),
                        Path::new(path),
                        crate::file_tab::Look {
                            family: "monospace".into(),
                            font_size: 14.,
                            theme: Default::default(),
                            autosave: false,
                        },
                        crate::i18n::Ui(guild_session::Language::English),
                        window,
                        cx,
                    )
                })
            };
            let plain = file("renamed.txt", cx, window);
            let source = file("justfile", cx, window);
            made = Some((root.clone(), plain, source));
            gpui_component::Root::new(root, window, cx)
        });
        cx.run_until_parked();
        let (root, plain, source) = made.unwrap();
        let target = cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                assert!(!source.read(cx).tasks().is_empty());
                root.note_gutter_menu("renamed.txt".into(), plain.clone(), cx);
                let plain_target = root.menu_target.take().unwrap();
                assert_eq!(
                    root.entries_for_target(&plain_target, cx).len(),
                    1,
                    "plain text offers only Blame"
                );
                root.note_gutter_menu("justfile".into(), source.clone(), cx);
                let target = root.menu_target.take().unwrap();
                assert_eq!(root.entries_for_target(&target, cx).len(), 2);
                target
            })
        });
        cx.update(|window, cx| {
            source.update(cx, |file, cx| {
                file.editor().unwrap().clone().update(cx, |editor, cx| {
                    editor.set_value("# no executable tasks\n", window, cx)
                });
                file.discover_tasks(cx);
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                assert!(source.read(cx).tasks().is_empty());
                assert_eq!(
                    root.entries_for_target(&target, cx).len(),
                    1,
                    "open target reflects updated discovery"
                );
                root.run_gutter_deed(GutterDeed::RunFile, &target, window, cx);
                assert!(
                    root.notices
                        .visible()
                        .any(|(_, n)| n.text == root.ui().text(Text::RunSourceChanged)),
                    "stale click explains why launch is unavailable"
                );
                assert!(root.runs.is_empty());
                root.run_gutter_deed(GutterDeed::Annotate, &target, window, cx);
                assert!(source.read(cx).blame_enabled());
                assert!(
                    !plain.read(cx).blame_enabled(),
                    "action retains the source entity"
                );
                root.stop_every_process(cx);
            })
        });
    }
}
