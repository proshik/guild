//! The tab's terminal is a cached view: a frame that changes nothing in it
//! replays its last paint, and everything a person does with it still
//! reaches it through that replay.
use crate::{Body, Root};
use gpui::{AppContext, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, px};
use std::time::Duration;

type Terminal = Entity<crate::terminal::view::TerminalTab>;

struct Fixture<'a> {
    _dir: tempfile::TempDir,
    root: Entity<Root>,
    terminal: Terminal,
    tab: crate::TabId,
    cx: &'a mut VisualTestContext,
}

impl Drop for Fixture<'_> {
    fn drop(&mut self) {
        self.terminal.update(self.cx, |view, cx| view.stop(cx));
        guild_session::store::flush();
    }
}

/// A project with one tab in front: a shell that says *ready* and then
/// echoes what it is given.
fn mount(cx: &mut TestAppContext) -> Fixture<'_> {
    mount_with(cx, "printf 'ready\\n'; exec /bin/cat")
}

/// [`mount`] with another script for the shell; it must print `ready`.
fn mount_with<'a>(cx: &'a mut TestAppContext, script: &str) -> Fixture<'a> {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(desk, crate::paths::state_dir(), cx);
            root.isolated_render = true;
            root
        });
        made = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    cx.simulate_resize(gpui::size(px(1100.), px(650.)));
    cx.run_until_parked();
    let root = made.unwrap();
    let terminal = cx.update(|_, cx| {
        let mut command = portable_pty::CommandBuilder::new("/bin/sh");
        command.args(["-c", script]);
        let mut look = crate::terminal::view::Look::of(&Default::default(), Default::default());
        look.copy_on_select = false;
        crate::terminal::view::open_job(command, std::path::Path::new("/"), look, cx).unwrap()
    });
    let tab = cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let seat = r.active_seat().unwrap();
            let session = r.desk.add_session(guild_session::SessionRecord::new(
                "Shell".into(),
                seat,
                std::path::PathBuf::from("/"),
                guild_session::Runs::Shell,
            ));
            let facts = terminal.read(cx).facts().clone();
            let id = r.adopt(
                seat,
                seat.project,
                Body::Terminal {
                    session,
                    view: terminal.clone(),
                    facts,
                },
                cx,
            );
            r.active_tab = Some(id);
            r.refocus(window, cx);
            cx.notify();
            id
        })
    });
    let mut f = Fixture {
        _dir: dir,
        root,
        terminal,
        tab,
        cx,
    };
    until(&mut f, "the shell's first line", |text| {
        text.contains("ready")
    });
    f
}

fn frames(f: &mut Fixture<'_>, n: usize) {
    for _ in 0..n {
        f.cx.executor().advance_clock(Duration::from_millis(17));
        f.cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        f.cx.run_until_parked();
    }
}

/// Frames until the terminal's text passes `done` — its output arrives
/// on a thread of its own.
fn until(f: &mut Fixture<'_>, what: &str, done: impl Fn(&str) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !done(&f.terminal.read_with(f.cx, |view, _| view.text_for_test())) {
        assert!(std::time::Instant::now() < deadline, "never saw {what}");
        std::thread::sleep(Duration::from_millis(5));
        frames(f, 1);
    }
    frames(f, 3);
}

fn paints(f: &mut Fixture<'_>) -> usize {
    f.terminal.read_with(f.cx, |view, _| view.paints)
}

/// Draw `n` frames in which only the root and the column changed.
fn unrelated(f: &mut Fixture<'_>, n: usize) {
    let column = f.root.update(f.cx, |r, cx| {
        r.region(crate::view::region::Part::Column, cx)
    });
    for _ in 0..n {
        f.root.update(f.cx, |_, cx| cx.notify());
        f.cx.run_until_parked();
        column.update(f.cx, |_, cx| cx.notify());
        f.cx.run_until_parked();
    }
}

/// The window in keyboard modality, the terminal holding the keys again.
/// GPUI's first key-down after the mouse switches the modality, which
/// refreshes the window and paints every cached view before the key is
/// dispatched — so a test that means to reach a replayed frame switches
/// first, with a key nothing takes while the column has the keys.
fn keyboard_modality(f: &mut Fixture<'_>) {
    let sidebar = f.root.read_with(f.cx, |r, _| r.sidebar_focus.clone());
    f.cx.update(|window, cx| window.focus(&sidebar, cx));
    f.cx.run_until_parked();
    f.cx.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("f19").unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
        assert!(window.last_input_was_keyboard());
    });
    f.cx.run_until_parked();
    let focus = focus_of(f);
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.run_until_parked();
}

/// `act` inside one update, with no frame drawn between it and the frame
/// before: how many times the grid was painted meanwhile (none, or the
/// frame was not the replayed one) and how many inputs reached it.
fn without_a_frame(
    f: &mut Fixture<'_>,
    act: impl FnOnce(&mut gpui::Window, &mut gpui::App),
) -> (usize, u64) {
    let terminal = f.terminal.clone();
    f.cx.update(|window, cx| {
        let read = |cx: &gpui::App| {
            let view = terminal.read(cx);
            (view.paints, view.input_epoch())
        };
        let before = read(cx);
        act(window, cx);
        let after = read(cx);
        (after.0 - before.0, after.1 - before.1)
    })
}

/// A printable key as the platform delivers its text — through the input
/// handler the last frame installed.
fn type_char(window: &mut gpui::Window, cx: &mut gpui::App, key: &str) {
    window.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), cx);
}

/// What one typed character adds to the input epoch: the key-down's
/// intent, and the platform's commit through the text client — only the
/// first without a client.
const TYPED: u64 = 2;

#[gpui::test]
fn terminal_body_replays_its_paint_and_still_hears_everything(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::terminal_body_replays_its_paint_and_still_hears_everything"
    )) {
        return;
    }
    let mut f = mount(cx);
    let terminal = f.terminal.clone();
    let focused = |f: &mut Fixture<'_>| {
        let focus = f.terminal.read_with(f.cx, gpui::Focusable::focus_handle);
        f.cx.update(|window, _| focus.is_focused(window))
    };
    assert!(focused(&mut f), "the tab in front has the keys");
    keyboard_modality(&mut f);

    // A notify of the root, or of the column — what a scroll of it and a
    // working ring's tick are — paints nothing of the grid.
    let before = paints(&mut f);
    unrelated(&mut f, 5);
    assert_eq!(paints(&mut f), before, "the grid is replayed, not painted");

    // Text through the platform's input handler that the replayed frame
    // installed: it reaches the program before anything is painted, and
    // its echo is painted.
    let (painted, typed) = without_a_frame(&mut f, |window, cx| type_char(window, cx, "h"));
    assert_eq!(
        (painted, typed),
        (0, TYPED),
        "typed through the replayed frame"
    );
    until(&mut f, "the echo", |text| text.contains("h"));
    assert!(paints(&mut f) > before, "output repaints");

    // A selection dragged past the grid's left edge, across frames the
    // root drew without the terminal: the window still hears the release.
    unrelated(&mut f, 2);
    let grid = terminal
        .read_with(f.cx, |view, _| view.bounds_for_test())
        .expect("painted");
    let start = gpui::point(grid.left() + px(40.), grid.top() + px(8.));
    f.cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    f.cx.run_until_parked();
    unrelated(&mut f, 2);
    let outside = gpui::point(grid.left() - px(30.), grid.top() + px(40.));
    f.cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::default());
    f.cx.run_until_parked();
    unrelated(&mut f, 2);
    f.cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
    f.cx.run_until_parked();
    terminal.read_with(f.cx, |view, _| {
        assert!(view.has_selection(), "the drag selected");
        assert!(
            !view.grabbing_for_test(),
            "the release past the edge ended it"
        );
    });

    // A theme, a font size and a window size each repaint it.
    let before = paints(&mut f);
    terminal.update(f.cx, |view, cx| {
        view.set_theme(crate::theme::Theme::light(), cx)
    });
    f.cx.run_until_parked();
    assert!(paints(&mut f) > before, "a theme repaints");
    let before = paints(&mut f);
    terminal.update(f.cx, |view, cx| view.set_font_size(17., cx));
    f.cx.run_until_parked();
    assert!(paints(&mut f) > before, "a font size repaints");
    let before = paints(&mut f);
    f.cx.simulate_resize(gpui::size(px(1000.), px(600.)));
    f.cx.run_until_parked();
    assert!(paints(&mut f) > before, "a new size repaints");

    // Focus moving away repaints it — the caret goes hollow — and a click
    // on the grid brings the keys back through the replayed listener.
    unrelated(&mut f, 2);
    let before = paints(&mut f);
    f.cx.update(|window, cx| {
        let sidebar = f.root.read(cx).sidebar_focus.clone();
        window.focus(&sidebar, cx);
    });
    f.cx.run_until_parked();
    assert!(!focused(&mut f));
    assert!(paints(&mut f) > before, "losing the focus repaints");
    unrelated(&mut f, 2);
    let grid = terminal
        .read_with(f.cx, |view, _| view.bounds_for_test())
        .expect("painted");
    f.cx.simulate_click(grid.center(), Modifiers::default());
    f.cx.run_until_parked();
    assert!(focused(&mut f), "a click on a replayed grid takes the keys");
}

fn focus_of(f: &mut Fixture<'_>) -> gpui::FocusHandle {
    f.terminal.read_with(f.cx, gpui::Focusable::focus_handle)
}

fn text_client(f: &mut Fixture<'_>) -> bool {
    f.terminal.read_with(f.cx, |view, _| view.text_client)
}

#[gpui::test]
fn terminal_body_enter_reaches_the_program_through_a_replayed_frame(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::terminal_body_enter_reaches_the_program_through_a_replayed_frame"
    )) {
        return;
    }
    // No echo, and one byte read raw: only a carriage return answers.
    let mut f = mount_with(
        cx,
        "stty raw -echo; printf 'ready\\n'; c=$(dd bs=1 count=1 2>/dev/null); \
         if [ \"$c\" = \"$(printf '\\r')\" ]; then printf 'GOT-CR'; else printf 'GOT-OTHER'; fi; \
         exec /bin/cat",
    );
    keyboard_modality(&mut f);
    let before = paints(&mut f);
    unrelated(&mut f, 3);
    assert_eq!(paints(&mut f), before, "replayed");
    // The key itself, as the platform delivers it: no text falls back to
    // the input handler if the terminal's own listener is gone, and the
    // listener heard is the replayed frame's.
    let (painted, sent) = without_a_frame(&mut f, |window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("enter").unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    });
    assert_eq!((painted, sent), (0, 1), "Enter through the replayed frame");
    until(&mut f, "the program's answer", |text| text.contains("GOT-"));
    let text = f.terminal.read_with(f.cx, |view, _| view.text_for_test());
    assert!(text.contains("GOT-CR"), "{text}");
}

#[gpui::test]
fn terminal_body_chosen_in_the_column_takes_text_in_its_first_frame(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::terminal_body_chosen_in_the_column_takes_text_in_its_first_frame"
    )) {
        return;
    }
    let mut f = mount(cx);
    // The tab listed under its worktree's row.
    f.root.update(f.cx, |r, cx| {
        let seat = r.active_seat().unwrap();
        let path = r.desk.project(seat.project).unwrap().path.clone();
        r.desk.toggle_worktree_tabs(seat.workspace, &path);
        cx.notify();
    });
    f.cx.run_until_parked();
    // The keys in the column: the grid is painted without a text client.
    let sidebar = f.root.read_with(f.cx, |r, _| r.sidebar_focus.clone());
    f.cx.update(|window, cx| window.focus(&sidebar, cx));
    f.cx.run_until_parked();
    assert!(!text_client(&mut f));
    unrelated(&mut f, 2);
    // The tab's row in the column: the press hands the keys over while
    // the root draws, and the frame it draws is the terminal's own.
    let row =
        f.cx.debug_bounds(Box::leak(format!("column-tab-{}", f.tab).into_boxed_str()))
            .expect("the tab's row");
    f.cx.simulate_click(row.center(), gpui::Modifiers::default());
    let focus = focus_of(&mut f);
    assert!(
        f.cx.update(|window, _| focus.is_focused(window)),
        "the keys moved"
    );
    assert!(
        text_client(&mut f),
        "the frame that moved them registered the platform's text client"
    );
    // The same hand-over with nothing else to repaint the grid: the root
    // moves the keys to the body in front while it draws — what a row
    // pressed, a tab chosen or a pane closed all ask for. In keyboard
    // modality, so that the text below reaches the frame it drew.
    keyboard_modality(&mut f);
    f.cx.update(|window, cx| window.focus(&sidebar, cx));
    f.cx.run_until_parked();
    unrelated(&mut f, 2);
    assert!(!text_client(&mut f));
    f.root.update(f.cx, |r, cx| {
        r.focusing_front = true;
        cx.notify();
    });
    f.cx.run_until_parked();
    assert!(f.cx.update(|window, _| focus.is_focused(window)));
    let (painted, typed) = without_a_frame(&mut f, |window, cx| type_char(window, cx, "x"));
    assert_eq!(
        (painted, typed),
        (0, TYPED),
        "the root's hand-over drew the grid with the platform's text client"
    );
}

#[cfg(all(feature = "adaptive-terminal-refresh", target_os = "macos"))]
#[gpui::test]
fn terminal_body_shown_again_paints_what_it_printed_hidden(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::terminal_body_shown_again_paints_what_it_printed_hidden"
    )) {
        return;
    }
    // The program prints on its own a while after it starts.
    let mut f = mount_with(
        cx,
        "printf 'ready\\n'; sleep 2; printf 'while hidden'; exec /bin/cat",
    );
    let terminal = f.terminal.clone();
    f.cx.update(|window, cx| {
        terminal.update(cx, |view, cx| {
            view.set_refresh_enabled(true, window, cx);
            view.set_surface_visible(true, window, cx);
        })
    });
    frames(&mut f, 3);
    // Hidden — covered, as its host reports — it prints and paints nothing.
    f.cx.update(|window, cx| {
        terminal.update(cx, |view, cx| view.set_surface_visible(false, window, cx))
    });
    frames(&mut f, 2);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !terminal.read_with(f.cx, |view, _| {
        view.text_for_test().contains("while hidden")
    }) {
        assert!(std::time::Instant::now() < deadline, "the echo never came");
        std::thread::sleep(Duration::from_millis(5));
        frames(&mut f, 1);
    }
    let before = paints(&mut f);
    // Shown again, with nothing else changing: the grid it holds is
    // painted, not the one replayed from before it was covered.
    f.cx.update(|window, cx| {
        terminal.update(cx, |view, cx| view.set_surface_visible(true, window, cx))
    });
    frames(&mut f, 2);
    unrelated(&mut f, 2);
    assert!(paints(&mut f) > before, "shown again, it paints");
}

#[cfg(all(feature = "adaptive-terminal-refresh", target_os = "macos"))]
#[gpui::test]
fn terminal_body_uncovered_window_paints_what_it_printed_covered(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::terminal_body_uncovered_window_paints_what_it_printed_covered"
    )) {
        return;
    }
    let mut f = mount_with(
        cx,
        "printf 'ready\\n'; sleep 2; printf 'while covered'; exec /bin/cat",
    );
    let terminal = f.terminal.clone();
    f.cx.update(|window, cx| {
        terminal.update(cx, |view, cx| {
            view.set_refresh_enabled(true, window, cx);
            view.set_surface_visible(true, window, cx);
            view.set_surface_visible(false, window, cx);
        })
    });
    frames(&mut f, 2);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !terminal.read_with(f.cx, |view, _| {
        view.text_for_test().contains("while covered")
    }) {
        assert!(
            std::time::Instant::now() < deadline,
            "the output never came"
        );
        std::thread::sleep(Duration::from_millis(5));
        frames(&mut f, 1);
    }
    // The host still reports the terminal shown — it is in front of its
    // tab — and what hid it was the window being covered: only the
    // window's visibility says it is drawable again. The test platform has
    // no native occlusion, so the host's answer is set without asking.
    terminal.update(f.cx, |view, _| view.set_host_shown_for_test(true));
    unrelated(&mut f, 2);
    let before = paints(&mut f);
    assert!(
        !terminal.read_with(f.cx, |view, _| view.painted_for_test()),
        "the grid on screen is the one from before"
    );
    f.cx.simulate_visibility_change(gpui::WindowVisibility::Hidden);
    f.cx.run_until_parked();
    f.cx.simulate_visibility_change(gpui::WindowVisibility::Visible);
    f.cx.run_until_parked();
    unrelated(&mut f, 2);
    assert!(paints(&mut f) > before, "uncovered, it paints");
}

#[gpui::test]
fn terminal_body_drops_a_refused_composition_from_the_screen(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::terminal_body_drops_a_refused_composition_from_the_screen"
    )) {
        return;
    }
    use gpui::EntityInputHandler as _;
    let mut f = mount(cx);
    let terminal = f.terminal.clone();
    f.cx.update(|window, cx| {
        terminal.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(None, "に", None, window, cx)
        })
    });
    f.cx.run_until_parked();
    // The program can take nothing more: the commit is refused.
    terminal.update(f.cx, |view, _| view.close_input_for_test());
    let before = paints(&mut f);
    f.cx.update(|window, cx| {
        terminal.update(cx, |view, cx| {
            view.replace_text_in_range(None, "に", window, cx)
        })
    });
    f.cx.run_until_parked();
    unrelated(&mut f, 2);
    assert!(
        terminal.read_with(f.cx, |view, _| !view.composing_for_test()),
        "the composition is gone"
    );
    assert!(paints(&mut f) > before, "and so is its underlined text");
}

#[gpui::test]
fn terminal_body_drops_a_composition_a_refused_paste_commits(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::terminal_body_drops_a_composition_a_refused_paste_commits"
    )) {
        return;
    }
    use gpui::EntityInputHandler as _;
    let mut f = mount(cx);
    let terminal = f.terminal.clone();
    f.cx.update(|window, cx| {
        terminal.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(None, "に", None, window, cx)
        })
    });
    f.cx.run_until_parked();
    // Paste commits the composition first; the program takes nothing, and
    // the clipboard has no text to paste after it.
    terminal.update(f.cx, |view, _| view.close_input_for_test());
    assert!(f.cx.update(|_, cx| cx.read_from_clipboard()).is_none());
    let before = paints(&mut f);
    f.cx.update(|window, cx| terminal.update(cx, |view, cx| view.paste_with_input(window, cx)));
    f.cx.run_until_parked();
    unrelated(&mut f, 2);
    assert!(terminal.read_with(f.cx, |view, _| !view.composing_for_test()));
    assert!(
        paints(&mut f) > before,
        "the underlined text left the screen"
    );
}
