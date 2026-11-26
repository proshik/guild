//! Disk snapshots are read away from GPUI and published only by their owner.
use super::*;

type Snapshot = (FileState, Option<(String, LineEnding)>);

#[derive(Default)]
pub(super) struct Loading {
    generation: u64,
    edits: u64,
    task: Option<gpui::Task<()>>,
    completed: Option<Snapshot>,
    pub window: Option<gpui::AnyWindowHandle>,
}
impl Loading {
    pub fn pending(&self) -> bool {
        self.task.is_some() || self.completed.is_some()
    }
    pub fn cancel(&mut self) {
        self.generation += 1;
        self.task = None;
        self.completed = None;
    }
}
impl FileTab {
    pub(super) fn load_text(
        &mut self,
        reconcile: bool,
        window: Option<&Window>,
        cx: &mut Context<Self>,
    ) {
        self.loading.cancel();
        self.pending_write = None;
        let generation = self.loading.generation;
        let edits = self.loading.edits;
        let path = self.path();
        self.loading.window = window.map(Window::window_handle).or(self.loading.window);
        let work = cx.background_executor().spawn(async move {
            let result = super::read(&path);
            let record = SaveRecord::of(&path).ok();
            (result, record)
        });
        self.loading.task = Some(cx.spawn(async move |owner, cx| {
            let (result, record) = work.await;
            let window = owner
                .update(cx, |tab, cx| {
                    if tab.loading.generation != generation || tab.relocating {
                        return None;
                    }
                    tab.loading.task = None;
                    let unchanged = result
                        .1
                        .as_ref()
                        .is_some_and(|(text, _)| text == &tab.clean);
                    if (reconcile && unchanged && (tab.editor.is_some() || tab.pending.is_some()))
                        || edits != tab.loading.edits
                        || (reconcile && tab.dirty)
                    {
                        if unchanged {
                            tab.saved = record;
                        } else {
                            tab.changed_on_disk = true;
                        }
                        if tab.dirty {
                            tab.arm_autosave(cx);
                        }
                    } else if tab.image.is_some() {
                        tab.load_image(true, cx);
                    } else if tab.editor.is_none() {
                        tab.state = result.0;
                        tab.pending = result.1.map(|(text, ending)| {
                            tab.line_ending = ending;
                            tab.clean = text.clone();
                            text
                        });
                        if tab.pending.is_none() {
                            tab.clean.clear();
                        }
                        tab.dirty = false;
                        tab.changed_on_disk = false;
                        tab.discover_tasks(cx);
                        cx.emit(BufferChanged);
                    } else {
                        tab.loading.completed = Some(result);
                    }
                    cx.notify();
                    tab.loading.window
                })
                .ok()
                .flatten();
            if let Some(window) = window {
                let _ = cx.update_window(window, |_, window, cx| {
                    owner.update(cx, |tab, cx| tab.realise(window, cx))
                });
            }
        }));
        cx.notify();
    }

    pub(super) fn realise_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((state, text)) = self.loading.completed.take() else {
            return;
        };
        self.install_read(state, text, window, cx);
    }

    pub(super) fn edited_during_read(&mut self) {
        self.loading.edits += 1;
        // A completed, unpainted result is just as obsolete as an outstanding one.
        if self.loading.completed.take().is_some() {
            self.changed_on_disk = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};

    #[gpui::test]
    fn reload_results_preserve_edits_saves_and_new_paths(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "original\r\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "relocated\n").unwrap();
        let mut made = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let file = cx.new(|cx| {
                FileTab::open(
                    dir.path(),
                    Path::new("a.txt"),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: Default::default(),
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    window,
                    cx,
                )
            });
            made = Some(file.clone());
            gpui_component::Root::new(file, window, cx)
        });
        cx.run_until_parked();
        let file = made.unwrap();
        std::fs::write(dir.path().join("a.txt"), "external\r\n").unwrap();
        cx.update(|window, cx| {
            file.update(cx, |file, cx| {
                file.reload(window, cx);
                let editor = file.editor.clone().unwrap();
                assert_eq!(
                    editor.read(cx).text().to_string(),
                    "original\n",
                    "reload returns before IO"
                );
                editor.update(cx, |e, cx| e.set_value("typed\n", window, cx));
                file.changed(&editor, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(file.read(cx).search_buffer(cx).unwrap().text, "typed\n");
            assert_eq!(file.read(cx).mark(), Some(Mark::Conflict));
        });
        cx.update(|window, cx| {
            file.update(cx, |file, cx| {
                file.reload(window, cx);
                file.save(cx).unwrap();
            })
        });
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(dir.path().join("a.txt")).unwrap(),
            b"typed\r\n"
        );
        cx.update(|window, cx| {
            file.update(cx, |file, cx| {
                assert_eq!(file.search_buffer(cx).unwrap().text, "typed\n");
                assert!(!file.is_dirty());
                file.reload(window, cx);
                file.relocating(true, window, cx);
                file.relocate(Path::new("b.txt"), window, cx);
                file.relocating(false, window, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(file.read(cx).search_buffer(cx).unwrap().text, "relocated\n"));
        std::fs::remove_file(dir.path().join("b.txt")).unwrap();
        cx.update(|window, cx| file.update(cx, |file, cx| file.reload(window, cx)));
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(file.read(cx).state, FileState::Gone);
            assert!(file.read(cx).editor.is_none());
            assert!(file.read(cx).pending.is_none());
        });
    }
}
