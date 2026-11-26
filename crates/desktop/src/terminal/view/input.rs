//! What the keyboard does.
//!
//! Keys become bytes for the program through `terminal::keys`; Copy and
//! Paste are the window's, not the program's. The pointer is in
//! `pointer`, a sibling since M5.12.

use super::TerminalTab;
use crate::terminal::{keys, scroll};
use gpui::{ClipboardItem, Context, ExternalPaths, InteractiveElement as _, KeyDownEvent};

impl TerminalTab {
    pub(crate) fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        self.dismiss_restore(cx);
        // Copy and Paste are not caught here since M5.12: `cmd-c` and
        // `cmd-v` are bindings, a binding is dispatched before any key
        // handler, and `bind_keys` answers their actions — a catch here
        // would never run and would only look alive.
        let stroke = &event.keystroke;
        // Fallback after configured GPUI bindings: custom effective shortcuts win.
        if stroke.modifiers.platform
            && !stroke.modifiers.control
            && !stroke.modifiers.alt
            && !stroke.modifiers.shift
            && matches!(stroke.key.as_str(), "up" | "down")
            && self.navigate_command(stroke.key == "up", cx)
        {
            cx.stop_propagation();
            return;
        }
        // Printable characters are delivered by the platform text client, including
        // dead keys, IME and Dictation. Never send key_char a second time.
        if !self.dispatch_meta
            && !stroke.modifiers.control
            && !stroke.modifiers.platform
            && stroke
                .key_char
                .as_ref()
                .is_some_and(|s| !s.is_empty() && s.chars().all(|c| !c.is_control()))
        {
            // Physical input revokes an in-flight managed submission before
            // platform composition/commit can complete. It is not proof of text.
            self.raw_input();
            self.prompt_input = true;
            self.uncertain_input = true;
            self.pending_meta = stroke.modifiers.alt.then(|| stroke.clone());
            return;
        }

        // The history keys move the grid and reach no program — M5.12
        // task 5. Asked before anything else sees the stroke: a page of
        // history is not a keystroke that returns to the bottom, and not an
        // interrupt. In the alternate screen `history_move` answers `None`
        // and the key goes on to the program as the key it is.
        if let Some(to) = scroll::history_move(stroke, self.emulator.mode()) {
            cx.stop_propagation();
            self.emulator.scroll_to(to);
            self.scroll.clear();
            // So the bar shows while the page goes by, as a wheel shows it.
            self.scrolled_at = Some(std::time::Instant::now());
            cx.notify();
            return;
        }

        if let Some(bytes) = keys::bytes_for(&event.keystroke, self.emulator.mode()) {
            cx.stop_propagation();
            if !self.send_visible(&bytes, cx) {
                return;
            }
            // An interrupt fires no hook, so the window has to see it here:
            // Esc or Ctrl-C typed into a tab stops whatever its agent was
            // doing. M5.10.
            if (stroke.key == "escape" && !stroke.modifiers.modified())
                || (stroke.key == "c" && stroke.modifiers.control && !stroke.modifiers.platform)
            {
                if let Some(probe) = &mut self.codex_probe {
                    probe.local_interrupt();
                }
                self.interrupted = Some(std::time::Instant::now());
            }

            // Typing clears the selection and returns to the latest
            // output: typing while looking at history is pointless — the
            // answer appears where the person is not looking.
            self.emulator.clear_selection();
            self.emulator.scroll_to_bottom();
            self.scroll.clear();
            // And it makes the caret solid: a cursor that blinks out
            // mid-word makes the keystroke look lost.
            self.caret.typed(std::time::Instant::now());
            self.raw_input();
            self.prompt_input = true;
            let enter = matches!(stroke.key.as_str(), "enter" | "return");
            if enter && bytes == b"\r" {
                self.draft_input = false;
                // Enter is a submission attempt, never proof of an empty input.
                self.uncertain_input = false;
                self.awaiting_prompt = true;
            } else if (enter && (bytes == b"\n" || bytes == b"\x1b\r"))
                || (!stroke.modifiers.control
                    && !stroke.modifiers.alt
                    && !stroke.modifiers.platform
                    && stroke.key_char.as_deref().is_some_and(|text| {
                        !text.is_empty()
                            && text.chars().all(|c| !c.is_control())
                            && text.as_bytes() == bytes
                    }))
            {
                self.draft_input = true;
            } else {
                // Navigation/interrupt/completion/deletion can affect a composer,
                // but do not prove that text was inserted or that a draft is empty.
                self.uncertain_input = true;
            }
            cx.notify();
        }
    }

    /// Paste from the clipboard — wrapped when the program asked for it,
    /// by `keys::pasted`.
    pub fn paste(&mut self, cx: &mut Context<Self>) {
        self.dismiss_restore(cx);
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };

        self.emulator.scroll_to_bottom();
        let Some(bytes) = keys::pasted_bounded(
            &text,
            self.emulator.mode(),
            crate::terminal::emulator::INPUT_BYTES,
        ) else {
            self.emulator.refuse_oversized_input();
            self.report_input_refusal(cx);
            return;
        };
        if !self.send_visible(&bytes, cx) {
            return;
        }
        self.raw_input();
        self.prompt_input = true;
        self.draft_input = true;
        cx.notify();
    }

    /// Files dropped from Finder or the screenshot in the corner of the
    /// screen: their paths, typed as a paste — `keys::dropped` says why in
    /// that form. What a program does with them is its own business;
    /// `claude` takes an image path for an image.
    pub(super) fn drop_paths(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        self.dismiss_restore(cx);
        let text = keys::dropped(paths.paths());
        if text.is_empty() {
            return;
        }
        let Some(bytes) = keys::pasted_bounded(
            &text,
            self.emulator.mode(),
            crate::terminal::emulator::INPUT_BYTES,
        ) else {
            self.emulator.refuse_oversized_input();
            self.report_input_refusal(cx);
            return;
        };
        if !self.send_visible(&bytes, cx) {
            return;
        }
        self.emulator.scroll_to_bottom();
        self.raw_input();
        self.prompt_input = true;
        self.draft_input = true;
        cx.notify();
    }

    /// Copy the selection.
    pub fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.emulator.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// What the keyboard's actions do on the terminal — M5.12, task 7:
    /// Edit's Copy and Paste, from `cmd-c`, `cmd-v` and the menu alike.
    ///
    /// **An action, not a caught keystroke.** The menu bar enables an item
    /// when its action has a handler on the focus path, so until these
    /// were here Edit's Copy and Paste were grey over a terminal while the
    /// keys worked. In a file the editor component's own `Copy` and `Paste`
    /// still win: they are bound in `Input`, deeper than ours — M5.11's
    /// decision 12. Copy with nothing selected copies nothing, and leaves
    /// the clipboard as it was.
    ///
    /// A drop of files is here as the pointer's paste, and it takes the
    /// focus as a click would: the next key goes where the path went.
    pub(super) fn bind_keys(&self, el: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        el.on_action(cx.listener(|tab, _: &crate::actions::Copy, _window, cx| tab.copy(cx)))
            .on_action(cx.listener(|tab, _: &crate::actions::Paste, window, cx| {
                tab.paste_with_input(window, cx)
            }))
            .on_drop(cx.listener(|tab, paths: &ExternalPaths, window, cx| {
                window.focus(&tab.focus, cx);
                let later = paths.clone();
                if tab.commit_before(window, cx, move |tab, _, cx| tab.drop_paths(&later, cx)) {
                    return;
                }
                tab.drop_paths(paths, cx)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{
        emulator::{Emulator, GridSize},
        view::Look,
    };
    use gpui::AppContext as _;
    fn refusal_fixture(
        cx: &mut gpui::TestAppContext,
    ) -> (
        tempfile::TempDir,
        gpui::Entity<crate::Root>,
        guild_session::ProjectId,
        guild_session::SessionId,
    ) {
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        let session = desk.add_session(guild_session::SessionRecord::new(
            "fixture".into(),
            guild_session::Seat { workspace, project },
            dir.path().into(),
            guild_session::Runs::Shell,
        ));
        let root = cx.new(|cx| {
            let mut root = crate::Root::diagnostic(desk, dir.path().into(), cx);
            root.active_project = Some(project);
            root.terminal_jobs = crate::terminal_job::LiveJobs::load(dir.path().join("jobs.json"));
            root
        });
        (dir, root, project, session)
    }
    fn oversized_paste(tab: &gpui::Entity<TerminalTab>, cx: &mut gpui::TestAppContext) {
        tab.update(cx, |tab, cx| {
            let before = (
                tab.input_epoch,
                tab.prompt_input,
                tab.draft_input,
                tab.awaiting_prompt,
                tab.programmatic_epoch,
            );
            cx.write_to_clipboard(ClipboardItem::new_string(
                "x".repeat(crate::terminal::emulator::INPUT_BYTES + 1),
            ));
            tab.paste(cx);
            assert_eq!(
                (
                    tab.input_epoch,
                    tab.prompt_input,
                    tab.draft_input,
                    tab.awaiting_prompt,
                    tab.programmatic_epoch
                ),
                before
            );
        });
    }
    fn assert_refusal_notice(root: &gpui::Entity<crate::Root>, cx: &mut gpui::TestAppContext) {
        root.update(cx, |root, cx| {
            root.poll_status(cx);
            let notices: Vec<_> = root.notices.visible().collect();
            assert_eq!(
                notices.len(),
                1,
                "human refusal must reach Root's visible warning"
            );
            assert_eq!(notices[0].1.level, crate::notify::Level::Warning);
            assert!(notices[0].1.text.contains("nothing was queued"));
        });
    }
    #[gpui::test]
    fn dock_input_refusal_reaches_root(cx: &mut gpui::TestAppContext) {
        let (dir, root, project, _) = refusal_fixture(cx);
        let tab = cx.update(|cx| {
            crate::terminal::view::open_command(
                portable_pty::CommandBuilder::new("/bin/cat"),
                dir.path(),
                Look::of(&Default::default(), Default::default()),
                None,
                cx,
            )
            .unwrap()
        });
        root.update(cx, |root, cx| {
            root.add_panel_terminal(project, tab.clone(), cx);
        });
        tab.update(cx, |_, cx| {
            root.update(cx, |root, cx| root.poll_status(cx));
        });
        oversized_paste(&tab, cx);
        assert_refusal_notice(&root, cx);
        tab.update(cx, |tab, _| tab.emulator.hang_up_for_quit());
    }
    #[gpui::test]
    fn run_output_input_refusal_reaches_root(cx: &mut gpui::TestAppContext) {
        let (dir, root, project, _) = refusal_fixture(cx);
        let tab = root.update(cx, |root, cx| {
            root.start_invocation(
                project,
                crate::run::tabs::Invocation {
                    cwd: dir.path().into(),
                    line: "exec /bin/cat".into(),
                    task: None,
                },
                cx,
            );
            root.runs[&project]
                .active()
                .unwrap()
                .value
                .view
                .as_ref()
                .unwrap()
                .clone()
        });
        tab.update(cx, |_, cx| {
            root.update(cx, |root, cx| root.poll_status(cx));
        });
        oversized_paste(&tab, cx);
        assert_refusal_notice(&root, cx);
        tab.update(cx, |tab, _| tab.emulator.hang_up_for_quit());
    }
    #[gpui::test]
    fn idle_refusal_delivery_does_not_read_retained_terminal_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let (dir, root, project, session) = refusal_fixture(cx);
        let mut retained = Vec::new();
        for index in 0..4 {
            let spec = serde_json::from_value(serde_json::json!({"version":1,"request_id":format!("idle-{index}"),"program":"/bin/sh","args":["-c","printf retained"],"cwd":dir.path(),"target":{"kind":"session","id":session}})).unwrap();
            let tab = root.update(cx, |root, cx| {
                let receipt = root
                    .terminal_jobs
                    .start(
                        session,
                        project,
                        &spec,
                        Look::of(&Default::default(), Default::default()),
                        cx,
                    )
                    .unwrap();
                let tab = root.terminal_jobs.terminal(&receipt.id).unwrap();
                crate::Root::hear_terminal_input(&tab, cx);
                tab
            });
            let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !tab.update(cx, |tab, _| tab.job_completion().is_some()) {
                assert!(std::time::Instant::now() < end);
                std::thread::sleep(std::time::Duration::from_millis(3));
            }
            root.update(cx, |root, cx| {
                root.terminal_jobs.poll(cx);
            });
            retained.push(tab);
        }
        for _ in 0..3 {
            for tab in &retained {
                // Borrow the retained entity while ticking Root: any new
                // refusal-discovery entity read fails at the GPUI boundary.
                tab.update(cx, |_, cx| {
                    root.update(cx, |root, cx| root.poll_status(cx));
                });
            }
        }
        oversized_paste(&retained[0], cx);
        assert_refusal_notice(&root, cx);
    }
    #[gpui::test]
    fn refusal_events_coalesce_and_do_not_retain_closed_owners(cx: &mut gpui::TestAppContext) {
        let (dir, root, project, _) = refusal_fixture(cx);
        let tab = cx.update(|cx| {
            crate::terminal::view::open_command(
                portable_pty::CommandBuilder::new("/bin/cat"),
                dir.path(),
                Look::of(&Default::default(), Default::default()),
                None,
                cx,
            )
            .unwrap()
        });
        let delivered = std::rc::Rc::new(std::cell::Cell::new(0));
        root.update(cx, |root, cx| {
            root.add_panel_terminal(project, tab.clone(), cx);
            let delivered = delivered.clone();
            cx.subscribe(&tab, move |_, _, _: &super::super::InputRefused, _| {
                delivered.set(delivered.get() + 1)
            })
            .detach();
        });
        let key = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        tab.update(cx, |tab, cx| {
            tab.emulator.hang_up_for_quit();
            for _ in 0..32 {
                tab.on_key(&key, cx);
            }
            assert!(tab.pending_input_refusal.is_some());
            assert_eq!(
                delivered.get(),
                0,
                "delivery must be deferred and coalesced"
            );
        });
        assert_eq!(
            delivered.get(),
            1,
            "one source must not accumulate an error backlog"
        );
        root.update(cx, |root, _| {
            assert_eq!(root.notices.visible().count(), 1);
            let id = root.notices.visible().next().unwrap().0;
            root.notices.dismiss(id);
        });
        let weak_view = tab.downgrade();
        cx.update(|cx| {
            tab.update(cx, |tab, cx| tab.on_key(&key, cx));
            root.update(cx, |root, _| {
                root.panel_terminals.remove(&project);
            });
        });
        root.update(cx, |root, _| {
            assert!(
                root.notices.is_empty(),
                "a cached handle must not keep a removed owner's delivery active"
            )
        });
        drop(tab);
        cx.run_until_parked();
        assert!(
            weak_view.upgrade().is_none(),
            "pending delivery/subscription retained a closed terminal"
        );
        root.update(cx, |root, _| {
            assert!(
                root.notices.is_empty(),
                "closing a source revokes its pending delivery"
            )
        });
        let kept = cx.update(|cx| {
            crate::terminal::view::open_command(
                portable_pty::CommandBuilder::new("/bin/cat"),
                dir.path(),
                Look::of(&Default::default(), Default::default()),
                None,
                cx,
            )
            .unwrap()
        });
        root.update(cx, |root, cx| {
            root.add_panel_terminal(project, kept.clone(), cx)
        });
        let weak_root = root.downgrade();
        drop(root);
        cx.run_until_parked();
        assert!(
            weak_root.upgrade().is_none(),
            "a retained terminal subscription retained Root"
        );
        kept.update(cx, |tab, cx| {
            tab.emulator.hang_up_for_quit();
            tab.on_key(&key, cx);
        });
    }
    #[gpui::test]
    fn refused_human_handlers_reach_existing_root_notices(cx: &mut gpui::TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::refused_human_handlers_reach_existing_root_notices"
        )) {
            return;
        }
        cx.executor().allow_parking();
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, std::path::Path::new("/"));
        let seat = guild_session::Seat { workspace, project };
        let session = desk.add_session(guild_session::SessionRecord::new(
            "fixture".into(),
            seat,
            "/".into(),
            guild_session::Runs::Shell,
        ));
        let root = cx.new(|cx| crate::Root::from_state(Default::default(), desk, cx));
        let fixture = tempfile::tempdir().unwrap();
        let exit = fixture.path().join("exit");
        let mut command = portable_pty::CommandBuilder::new("/usr/bin/python3");
        command.args(["-I", "-u", "-c", "import os,tty,time,signal,pathlib,sys;signal.alarm(15);tty.setraw(0);os.write(1,b'\\x1b[?2004h\\x1b[?1000h')\nwhile not pathlib.Path(sys.argv[1]).exists():time.sleep(.003)"]);
        command.arg(&exit);
        let emulator =
            Emulator::spawn_job(command, std::path::Path::new("/"), GridSize::new(80, 24)).unwrap();
        let tab = cx.new(|cx| {
            TerminalTab::new(
                emulator,
                Look::of(&Default::default(), Default::default()),
                "/".into(),
                None,
                cx,
            )
        });
        root.update(cx, |root, cx| {
            crate::Root::hear_terminal_input(&tab, cx);
            root.adopt(
                seat,
                project,
                crate::Body::Terminal {
                    session,
                    view: tab.clone(),
                    facts: tab.read(cx).facts().clone(),
                },
                cx,
            );
        });
        // Use the existing real full-writer fixture and GPUI's fake clipboard.
        let end = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while !tab.update(cx, |tab, _| tab.accepts_submission()) {
            assert!(std::time::Instant::now() < end);
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        tab.update(cx, |tab, cx| {
            assert!(
                tab.emulator
                    .send(&vec![b'x'; crate::terminal::emulator::INPUT_BYTES])
            );
            assert!(!tab.submit("managed refusal", cx));
        });
        root.update(cx, |root, cx| {
            root.poll_status(cx);
            assert!(
                root.notices.is_empty(),
                "managed submit must not duplicate its returned error"
            );
        });
        for action in ["key", "paste", "oversized paste", "drop", "mouse", "reply"] {
            tab.update(cx, |tab, cx| {
                let epoch = tab.input_epoch();
                match action {
                    "key" => tab.on_key(
                        &gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("enter").unwrap(),
                            is_held: false,
                            prefer_character_input: false,
                        },
                        cx,
                    ),
                    "paste" => {
                        cx.write_to_clipboard(ClipboardItem::new_string("refused".into()));
                        tab.paste(cx);
                    }
                    "oversized paste" => {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            "x".repeat(crate::terminal::emulator::INPUT_BYTES),
                        ));
                        tab.paste(cx);
                    }
                    "drop" => tab.drop_paths(&ExternalPaths(vec!["/refused".into()].into()), cx),
                    "mouse" => {
                        tab.cell = Some(gpui::size(gpui::px(8.), gpui::px(16.)));
                        tab.bounds = Some(gpui::Bounds::new(
                            gpui::point(gpui::px(0.), gpui::px(0.)),
                            gpui::size(gpui::px(640.), gpui::px(384.)),
                        ));
                        tab.on_mouse_down(
                            &gpui::MouseDownEvent {
                                position: gpui::point(gpui::px(20.), gpui::px(20.)),
                                click_count: 1,
                                ..Default::default()
                            },
                            cx,
                        );
                    }
                    _ => tab.protocol_text_for_test("refused".into(), cx),
                }
                assert_eq!(tab.input_epoch(), epoch);
                assert!(!tab.has_draft());
                assert!(!tab.prompt_input);
                assert!(!tab.awaiting_prompt);
                assert!(!tab.uncertain_input);
            });
            root.update(cx, |root, cx| {
                root.poll_status(cx);
                let notices: Vec<_> = root.notices.visible().collect();
                assert_eq!(
                    notices.len(),
                    1,
                    "{action} refusal never reached visible notices"
                );
                assert_eq!(notices[0].1.level, crate::notify::Level::Warning);
                assert!(notices[0].1.text.contains("nothing was queued"));
                let id = notices[0].0;
                root.notices.dismiss(id);
            });
        }
        std::fs::write(exit, "").unwrap();
        while !tab.update(cx, |tab, _| tab.emulator.input_delivery_failed()) {
            assert!(std::time::Instant::now() < end);
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        tab.update(cx, |tab, cx| {
            let epoch = tab.input_epoch();
            tab.on_key(
                &gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("enter").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                cx,
            );
            assert_eq!(tab.input_epoch(), epoch);
            assert!(!tab.has_draft());
            assert!(
                tab.input_pending(),
                "admitted IO failure must remain uncertain"
            );
        });
        root.update(cx, |root, cx| {
            root.poll_status(cx);
            let (_, notice) = root.notices.visible().next().unwrap();
            assert_eq!(notice.level, crate::notify::Level::Warning);
            assert!(notice.text.contains("previously queued input is uncertain"));
        });
        tab.update(cx, |tab, _| tab.cancel_job());
    }
    #[gpui::test]
    fn input_keys_preserve_drafts_without_inventing_text(cx: &mut gpui::TestAppContext) {
        for name in [
            "escape",
            "ctrl-c",
            "up",
            "right",
            "tab",
            "delete",
            "backspace",
            "alt-b",
        ] {
            let emulator = Emulator::spawn_job(
                portable_pty::CommandBuilder::new("/bin/cat"),
                std::path::Path::new("/"),
                GridSize::new(80, 24),
            )
            .unwrap();
            let tab = cx.new(|cx| {
                TerminalTab::new(
                    emulator,
                    Look::of(&Default::default(), Default::default()),
                    std::path::PathBuf::from("/"),
                    None,
                    cx,
                )
            });
            tab.update(cx, |tab, cx| {
                let mut stroke = gpui::Keystroke::parse(name).unwrap();
                if name == "ctrl-c" {
                    stroke.key_char = Some("c".into());
                }
                if name == "alt-b" {
                    stroke.key_char = Some("∫".into());
                }
                let event = gpui::KeyDownEvent {
                    keystroke: stroke,
                    is_held: false,
                    prefer_character_input: false,
                };
                assert!(keys::bytes_for(&event.keystroke, tab.emulator.mode()).is_some());
                tab.on_key(&event, cx);
                tab.send_pending_meta(cx);
                assert!(!tab.has_draft(), "{name} invented an unsent draft");
                assert!(
                    !tab.initial_prompt(),
                    "{name} must still withhold automatic origin input"
                );
                assert!(
                    tab.input_pending(),
                    "{name} must withhold automatic correction input too"
                );
                // Printable platform commits use the text client, while meta/control
                // gestures retain their byte encoding and provenance behavior.
                tab.platform_insert("т", cx);
                assert!(tab.has_draft());
                tab.on_key(&event, cx);
                assert!(tab.has_draft(), "{name} erased a real draft");
                tab.cancel_job();
            });
        }
    }
    #[gpui::test]
    fn modified_enter_inserts_newline_without_submitting(cx: &mut gpui::TestAppContext) {
        let emulator = Emulator::spawn_job(
            portable_pty::CommandBuilder::new("/bin/cat"),
            std::path::Path::new("/"),
            GridSize::new(80, 24),
        )
        .unwrap();
        let tab = cx.new(|cx| {
            TerminalTab::new(
                emulator,
                Look::of(&Default::default(), Default::default()),
                std::path::PathBuf::from("/"),
                None,
                cx,
            )
        });
        tab.update(cx, |tab, cx| {
            for (name, expected) in [
                ("shift-enter", b"\n".as_slice()),
                ("alt-enter", b"\x1b\r".as_slice()),
            ] {
                let event = gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse(name).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                };
                assert_eq!(
                    keys::bytes_for(&event.keystroke, tab.emulator.mode()).unwrap(),
                    expected
                );
                tab.on_key(&event, cx);
                assert!(tab.has_draft(), "{name} cleared multiline unsent input");
                assert!(!tab.awaiting_prompt, "{name} invented a submission");
            }
            tab.cancel_job();
        });
    }
    #[gpui::test]
    fn cr_enter_tracks_submission_by_transmitted_bytes(cx: &mut gpui::TestAppContext) {
        let emulator = Emulator::spawn_job(
            portable_pty::CommandBuilder::new("/bin/cat"),
            std::path::Path::new("/"),
            GridSize::new(80, 24),
        )
        .unwrap();
        let tab = cx.new(|cx| {
            TerminalTab::new(
                emulator,
                Look::of(&Default::default(), Default::default()),
                std::path::PathBuf::from("/"),
                None,
                cx,
            )
        });
        tab.update(cx, |tab, cx| {
            for name in ["ctrl-enter", "cmd-enter", "fn-enter"] {
                let mut text = gpui::Keystroke::parse("t").unwrap();
                text.key_char = Some("text".into());
                tab.on_key(
                    &gpui::KeyDownEvent {
                        keystroke: text,
                        is_held: false,
                        prefer_character_input: false,
                    },
                    cx,
                );
                let event = gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse(name).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                };
                assert_eq!(
                    keys::bytes_for(&event.keystroke, tab.emulator.mode()).unwrap(),
                    b"\r"
                );
                tab.on_key(&event, cx);
                assert!(
                    !tab.has_draft(),
                    "{name} sent CR but kept a false unsent draft"
                );
                assert!(tab.awaiting_prompt);
                assert!(
                    tab.input_pending(),
                    "CR alone must not qualify workflow readiness"
                );
                assert!(!tab.uncertain_input);
            }
            tab.cancel_job();
        });
    }
    #[gpui::test]
    fn generic_submit_keeps_transport_semantics_after_unacknowledged_enter(
        cx: &mut gpui::TestAppContext,
    ) {
        let own = tempfile::tempdir().unwrap();
        let capture = own.path().join("stdin");
        let mut command = portable_pty::CommandBuilder::new("/usr/bin/python3");
        command.args([
            "-I",
            "-u",
            "-c",
            r#"import os,sys,tty
import pathlib
capture=pathlib.Path(sys.argv[1])
tty.setraw(0)
os.write(1,b"\x1b[?2004h")
data=b""
while b"generic caller" not in data or not data.endswith(b"\r"):
 data+=os.read(0,65536)
pending=capture.with_suffix(".pending")
pending.write_bytes(data)
os.replace(pending,capture)
"#,
        ]);
        command.arg(&capture);
        let emulator =
            Emulator::spawn_job(command, std::path::Path::new("/"), GridSize::new(80, 24)).unwrap();
        let tab = cx.new(|cx| {
            TerminalTab::new(
                emulator,
                Look::of(&Default::default(), Default::default()),
                std::path::PathBuf::from("/"),
                None,
                cx,
            )
        });
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !tab.update(cx, |v, _| v.accepts_submission()) {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        tab.update(cx, |v, cx| {
            v.on_key(
                &gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("enter").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                cx,
            );
            assert!(v.input_pending());
            assert!(!v.has_draft());
            assert!(
                v.submit("generic caller", cx),
                "workflow pending fact accidentally denied generic transport"
            );
            assert!(
                v.input_pending(),
                "generic send is not proof of empty workflow input"
            );
        });
        while !capture.exists() {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let mut expected = b"\r".to_vec();
        expected.extend(keys::submitted("generic caller"));
        assert_eq!(std::fs::read(&capture).unwrap(), expected);
        tab.update(cx, |v, _| v.cancel_job());
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    use crate::terminal::{
        emulator::{Emulator, GridSize},
        view::Look,
    };
    use gpui::AppContext as _;
    #[gpui::test]
    fn full_real_writer_refuses_managed_and_key_input_without_claiming_submission(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let mut cmd = portable_pty::CommandBuilder::new("/usr/bin/python3");
        cmd.args(["-I","-u","-c","import os,tty,time,signal;signal.alarm(15);tty.setraw(0);os.write(1,b'\\x1b[?2004h');time.sleep(12)"]);
        let emulator = Emulator::spawn_job(cmd, dir.path(), GridSize::new(80, 24)).unwrap();
        let tab = cx.new(|cx| {
            TerminalTab::new(
                emulator,
                Look::of(&Default::default(), Default::default()),
                dir.path().into(),
                None,
                cx,
            )
        });
        let end = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while !tab.update(cx, |t, _| t.accepts_submission()) {
            assert!(std::time::Instant::now() < end);
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        tab.update(cx, |t, cx| {
            let before = (
                t.input_epoch,
                t.prompt_input,
                t.draft_input,
                t.awaiting_prompt,
                t.programmatic_epoch,
            );
            assert!(!t.submit(&"x".repeat(8 * 1024 * 1024), cx));
            assert_eq!(
                (
                    t.input_epoch,
                    t.prompt_input,
                    t.draft_input,
                    t.awaiting_prompt,
                    t.programmatic_epoch
                ),
                before
            );
            assert!(t.emulator.send(&vec![b'x'; 8 * 1024 * 1024]));
            assert!(!t.submit("must remain unsent", cx));
            t.on_key(
                &gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("enter").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                cx,
            );
            assert_eq!(
                (
                    t.input_epoch,
                    t.prompt_input,
                    t.draft_input,
                    t.awaiting_prompt,
                    t.programmatic_epoch
                ),
                before,
                "refused key/submit mutated input facts"
            );
            t.cancel_job();
        });
    }
}
