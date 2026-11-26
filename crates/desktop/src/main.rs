//! The Guild window's entry point.
//!
//! Everything else is the library beside this file. What stays here is what
//! only an executable does: start logging, install the panic hook, open a
//! window, and be the one place `anyhow` is allowed.

use gpui::prelude::*;
use gpui::{App, Bounds, WindowBounds, WindowOptions, px, size};
use guild_session::{Settings, Store};

use guild_desktop::i18n::Ui;
use guild_desktop::{Root, actions, load_desk, logging, menu, paths};

fn main() -> anyhow::Result<()> {
    let private_args: Vec<_> = std::env::args_os().skip(1).collect();
    if let Some(result) = guild_desktop::update::helper::entry(&private_args) {
        return result.map_err(|failure| anyhow::anyhow!("update helper stopped: {failure:?}"));
    }
    if std::env::args_os()
        .skip(1)
        .any(|arg| !arg.to_string_lossy().starts_with("-psn_"))
    {
        let cli = std::env::current_exe()?.with_file_name("guild");
        anyhow::bail!(
            "guild-desktop opens the native window and accepts no CLI commands. Use {} for instructions and status commands; GUILD_SOCKET selects the target window.",
            cli.display()
        );
    }
    logging::start();
    logging::install_panic_hook();
    // Before anything opens a file it keeps: the Dock gives 256, and thirty
    // agent sessions use them up. After logging, so a refusal is written down.
    guild_desktop::system::open_files::raise();
    // Cache this instance's identity before any window can initiate another update.
    let update_startup =
        match guild_session::update::transaction::Startup::capture(paths::state_dir()) {
            Ok(startup) => startup,
            Err(failure) => {
                tracing::warn!(?failure, "update startup context was rejected");
                None
            }
        };
    let running_update_info = std::env::current_exe()
        .ok()
        .and_then(|exe| guild_desktop::update::running_bundle(&exe))
        .and_then(|bundle| guild_session::update::bundle::info(&bundle).ok());
    // Before `Application::new`, so the login shell is already answering
    // while the window is built: a restored tab spawns in `Root::new`.
    // M5.11's decision 22.
    guild_session::login_path::start();

    gpui_kit::application()
        .with_assets(guild_desktop::icons::Icons)
        .run(|cx: &mut App| {
            // **First, before anything is measured or drawn.** A face
            // registered after the first frame is a first frame in
            // somebody else's font, and the jump is visible.
            guild_desktop::fonts::register(cx);
            // The editor's component: its own init, our grammars, its bindings
            // undone where ours read the keys.
            //
            // **Before the keymap, and the order matters.** `editor::init`
            // takes what is bound when it ends as the inherited bindings,
            // and `actions::install` lays Guild's own over and under them —
            // the other way round, the component's `cmd-[` would be bound
            // after our Back and win in a buffer, and the first rebind from
            // Settings would clear the component's keys for good. M5.11's
            // decision 12.
            guild_desktop::editor::init(cx);
            actions::install(cx);
            // The Dock's Quit, a logout and a restart ask what `cmd-q` asks.
            // Inside `run`, which is after GPUI has set its application
            // delegate — the class the method is added to. M5.11's
            // decision 20.
            guild_desktop::system::install_should_terminate();
            // The language is needed before the window: the menu bar is drawn
            // by the system and is built before anything appears on screen.
            let settings = Store::<Settings>::new(paths::settings_file())
                .load()
                .unwrap_or_default();
            // The component draws with our palette from the first frame.
            guild_desktop::editor::apply_theme(
                &guild_desktop::theme::in_effect(
                    &settings,
                    guild_desktop::theme::system_dark(cx.window_appearance()),
                ),
                settings.ui_font_size,
                cx,
            );
            guild_desktop::editor::apply_ui_font(settings.ui_font.as_deref(), cx);
            // The menu is built after the keymap: the system takes from it
            // the shortcuts it shows next to the items. The recent list comes
            // from the state file — Root has not been created yet, and an
            // empty "Open Recent" on every launch would be a lie.
            let recent = load_desk();
            menu::install(
                Ui(settings.language),
                &recent.recent().collect::<Vec<_>>(),
                cx,
            );

            // The window opens where it was left. The remembered place is
            // validated: a monitor may have been unplugged, and a window past
            // the screen edge looks like an application that failed to start —
            // there is an icon in the Dock but no window.
            let display = cx
                .displays()
                .into_iter()
                .find(|display| {
                    settings.window.is_some_and(|place| {
                        display
                            .bounds()
                            .contains(&gpui::point(px(place.x), px(place.y)))
                    })
                })
                .or_else(|| cx.primary_display());
            let display_bounds = display
                .as_ref()
                .map(|d| d.visible_bounds())
                .unwrap_or_else(|| Bounds::centered(None, size(px(1180.0), px(760.0)), cx));
            let bounds = guild_desktop::frame::restored_bounds(
                settings.window,
                display_bounds,
                settings.ui_font_size,
            );
            let minimum = guild_desktop::frame::minimum(settings.ui_font_size, 0.0, 0.0);

            // Before the window, so that the Quit handler below can hold it
            // as well as the window's own closure.
            let root = cx.new(|cx| {
                let mut root = Root::new(cx);
                root.set_sound_player(guild_desktop::system::play_sound);
                root.set_update_startup(update_startup);
                root.set_running_update_info(running_update_info);
                if let Err(error) = root.enable_updater() {
                    tracing::warn!(kind = ?error.kind(), "updater worker could not start");
                }
                root
            });
            let handle = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        window_min_size: Some(minimum),
                        display_id: display.as_ref().map(|d| d.id()),
                        // The title bar is ours to draw in. The system's is an
                        // empty strip the height of a row of tabs, and in an
                        // environment that row is worth having: the project
                        // and its branch live there.
                        //
                        // Taking it means placing the traffic lights by hand.
                        // GPUI puts the top of each button's frame here, and
                        // the frame is 14 high — measured through AppKit on
                        // macOS 26.5 on 2026-09-24, when the person saw the
                        // bar's buttons sit above the lights: this said 12
                        // and put them at 11, a point under the centre line
                        // everything else in the 34-point bar is on. Since
                        // the same day the bar's contents stand a point
                        // below that line, optically, and the lights with
                        // them — `space::TRAFFIC_LIGHTS_TOP`.
                        app_owns_titlebar_drag: true,
                        titlebar: Some(gpui::TitlebarOptions {
                            title: Some("Guild".into()),
                            appears_transparent: true,
                            traffic_light_position: Some(gpui::point(
                                px(12.0),
                                px(guild_desktop::theme::space::TRAFFIC_LIGHTS_TOP),
                            )),
                        }),
                        ..Default::default()
                    },
                    {
                        let root = root.clone();
                        move |window, cx| {
                            // Quitting loses what is not written — an edited file,
                            // a turn in progress — and losing it silently is not
                            // acceptable. The window asks, one question per thing
                            // at stake, and closes on the answers rather than in
                            // advance of them.
                            let asking = root.clone();
                            window.on_window_should_close(cx, move |window, cx| {
                                asking.update(cx, |root, cx| root.may_close_window(window, cx))
                            });

                            // The component's root has to be the window's root
                            // view — `Root::read` expects it — so ours is its
                            // child. Task 0 of M8.1 found no other way in.
                            let view: gpui::AnyView = root.into();
                            cx.new(|cx| gpui_component::Root::new(view, window, cx))
                        }
                    },
                )
                .expect("the window opens");

            // cmd-q and the menu's Quit ask the same questions as the close
            // button. A global action listener runs before anything in the
            // window, so the window cannot intercept Quit on its own: it is
            // brought in here, by its handle. With no window left there is
            // nothing to ask, and the application quits.
            //
            // Deferred, because the action arrives while the window is being
            // updated, and a window cannot be updated from inside its own
            // update: the handle answers "not found", which read as "no
            // window" and quit without a word.
            let quitting = root.clone();
            cx.on_action(move |_: &actions::Quit, cx: &mut App| {
                let quitting = quitting.clone();
                cx.defer(move |cx| {
                    let asked = handle.update(cx, |_, window, cx| {
                        quitting.update(cx, |root, cx| root.may_close_window(window, cx))
                    });
                    match asked {
                        Ok(true) => guild_desktop::quit(cx),
                        Ok(false) => {}
                        Err(err) => {
                            tracing::warn!(%err, "no window to ask before quitting");
                            guild_desktop::quit(cx);
                        }
                    }
                });
            });

            // Raise the window itself, not only the application.
            //
            // `cx.activate` makes Guild the frontmost application — the icon
            // appears in Cmd+Tab — but it does not order any particular window
            // in front. Launched from a terminal, or with a full-screen space
            // in the way, that leaves an application one can switch to and a
            // window one cannot see. Which is exactly what it looked like.
            //
            // The bounds go into the log next to it: when a window is missing,
            // the first question is whether it was asked for somewhere it
            // could be seen, and guessing at that from outside the process is
            // what wasted the time here.
            let raised = handle.update(cx, |_, window, _| {
                window.activate_window();
                format!(
                    "{:?} active={}",
                    window.window_bounds(),
                    window.is_window_active()
                )
            });
            match raised {
                Ok(state) => tracing::info!(window = %state, "window opened and raised"),
                // No notice here, deliberately. This error means the window
                // handle is no longer valid — there is no window left to show a
                // notice in. The log is the only place it can go.
                Err(err) => tracing::error!(%err, "the window could not be raised"),
            }

            // Closing the window quits the application.
            //
            // Without this it stays alive with no window at all: the icon is
            // still in Cmd+Tab, switching to it shows nothing, and there is no
            // way to get a window back — while the sessions inside go on
            // running and spending money out of sight. That is not a state to
            // leave anyone in, and it is the state this reached.
            //
            // Quitting is also what the window already promises: the warning
            // shown when sessions are running says the work will stop. Closing
            // the window and staying alive made that warning a lie.
            //
            // **The main window decides, not the count** — M8.3.6: a Push
            // window left open would otherwise keep an application alive
            // whose main window is gone. The quit closes what is left.
            let main = handle.window_id();
            cx.on_window_closed(move |cx, _| {
                let remaining: Vec<_> = cx.windows().iter().map(|w| w.window_id()).collect();
                if guild_desktop::windows::quits(main, &remaining) {
                    tracing::info!("the main window is closed, quitting");
                    guild_desktop::quit(cx);
                }
            })
            .detach();

            cx.activate(true);
        });

    Ok(())
}
